import ExpCore
import ExpUI
import Foundation
import GRDB

/// #FEED-237 (message-delivery.json): a person's message from the press of
/// Send until its synced row takes its place. The entries live in the
/// account's GRDB `outbox` table (an app restart keeps every one); the
/// sends run on this process-wide store, so backing out of the screen never
/// cancels one. A send carries the entry's clientId as `clientMessageId`
/// (F21: a resend returns the row the server already stored); success
/// stamps the messageId, a failure flips the entry to `failed` (no toast:
/// the bubble's red mark says so).
@MainActor
final class SessionOutbox {
    static let shared = SessionOutbox()

    /// The accounts whose `sending` entries were resent this launch.
    private var resumed = Set<String>()
    /// The entries with a send out right now (a resend never doubles one).
    private var inFlight = Set<String>()

    private static let iso: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    /// Put a fresh entry at the bottom and send it.
    func send(
        accountId: String, sessionId: String, text: String, attachmentIds: [String],
        scope: TeamsScope?, db: DatabaseManager, api: SessionsApi
    ) {
        let entry = MessageOutboxEntity(
            clientId: UUID().uuidString.lowercased(), sessionId: sessionId, text: text,
            createdAt: Self.iso.string(from: Date()), attachmentIds: attachmentIds
        )
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        try? pool.write { db in try entry.insert(db) }
        deliver(entry, accountId: accountId, scope: scope, db: db, api: api)
    }

    /// A failed bubble's Retry: the SAME clientId, back to `sending`.
    func retry(accountId: String, clientId: String, db: DatabaseManager, api: SessionsApi) {
        guard let pool = try? db.pool(forAccountId: accountId),
              var entry = try? pool.read({ db in try MessageOutboxEntity.fetchOne(db, key: clientId) })
        else { return }
        entry.state = MessageDelivery.LocalState.sending.rawValue
        try? pool.write { [entry] db in try entry.update(db) }
        deliver(entry, accountId: accountId, scope: nil, db: db, api: api)
    }

    /// A failed bubble's Delete, or an entry its synced row replaced.
    func remove(accountId: String, clientIds: [String], db: DatabaseManager) {
        guard !clientIds.isEmpty, let pool = try? db.pool(forAccountId: accountId) else { return }
        _ = try? pool.write { db in try MessageOutboxEntity.deleteAll(db, keys: clientIds) }
    }

    /// On start: every `sending` entry without a messageId goes out again
    /// with its clientMessageId (once per account per launch); a `failed`
    /// one waits for its Retry.
    func resume(accountId: String, db: DatabaseManager, api: SessionsApi) {
        guard resumed.insert(accountId).inserted, let pool = try? db.pool(forAccountId: accountId) else { return }
        let pending = (try? pool.read { db in
            try MessageOutboxEntity
                .filter(Column("state") == MessageDelivery.LocalState.sending.rawValue)
                .filter(Column("message_id") == nil)
                .order(Column("created_at"))
                .fetchAll(db)
        }) ?? []
        for entry in pending {
            deliver(entry, accountId: accountId, scope: nil, db: db, api: api)
        }
    }

    private func deliver(
        _ entry: MessageOutboxEntity, accountId: String, scope: TeamsScope?, db: DatabaseManager, api: SessionsApi
    ) {
        guard inFlight.insert(entry.clientId).inserted else { return }
        let attachmentIds = entry.attachmentIdList
        Task { [weak self] in
            var messageId: String?
            var failed = false
            do {
                let result = try await api.send(
                    accountId: accountId,
                    .message(
                        sessionId: entry.sessionId, text: entry.text.isEmpty ? nil : entry.text,
                        attachmentIds: attachmentIds.isEmpty ? nil : attachmentIds, scope: scope
                    ),
                    clientMessageId: entry.clientId
                )
                messageId = result.messageId
            } catch {
                failed = true
            }
            self?.inFlight.remove(entry.clientId)
            guard let pool = try? db.pool(forAccountId: accountId) else { return }
            let stored = messageId
            let refused = failed
            try? await pool.write { db in
                // Deleted meanwhile (a Delete, its row replaced it): nothing to stamp.
                guard var current = try MessageOutboxEntity.fetchOne(db, key: entry.clientId) else { return }
                if refused {
                    current.state = MessageDelivery.LocalState.failed.rawValue
                } else {
                    current.messageId = stored
                }
                try current.update(db)
            }
        }
    }
}

/// #FEED-237: one of the viewer's own bubbles at the transcript's bottom:
/// a synced row the device has not consumed, or an outbox entry.
struct OwnPendingBubble: Identifiable, Equatable {
    /// The row's id, or the entry's clientId.
    let id: String
    let text: String
    let at: Date?
    let state: MessageDelivery.State
    /// The server holds the row (the menu's Cancel sending).
    let stored: Bool
    /// The entry (nil = a synced row alone).
    let clientId: String?
    /// The row to unqueue (nil = not stored yet).
    let messageId: String?

    var actions: [MessageDelivery.Action] {
        MessageDelivery.bubbleActions(state: state, stored: stored)
    }

    /// The bottom of the transcript (message-delivery.json ORDER): the
    /// viewer's own synced rows the device has not consumed, oldest first,
    /// then the kept outbox entries; `drop` = the entries a row replaced.
    static func build(
        viewerId: String, rows: [MessageEntity], outbox: [MessageOutboxEntity]
    ) -> (bubbles: [OwnPendingBubble], drop: [String]) {
        let merge = MessageDelivery.outboxMerge(
            viewerId: viewerId,
            outbox: outbox.map(\.entry),
            rows: rows.map { row in
                MessageDelivery.OutboxRow(
                    id: row.id, authorKind: row.authorKind, authorId: row.authorId, body: row.body,
                    createdAt: row.createdAt
                )
            }
        )
        let pending = rows
            .filter { $0.authorKind == "user" && $0.authorId == viewerId && $0.consumedAt == nil }
            .sorted { a, b in
                let at = WireTimestamps.parse(a.createdAt) ?? .distantPast
                let bt = WireTimestamps.parse(b.createdAt) ?? .distantPast
                return at != bt ? at < bt : a.id < b.id
            }
            .map { row in
                OwnPendingBubble(
                    id: row.id, text: row.body, at: WireTimestamps.parse(row.createdAt),
                    state: MessageDelivery.deliveryState(MessageDelivery.StateInput(
                        local: nil, stored: true, deliveredAt: row.deliveredAt, consumedAt: row.consumedAt, echoed: false
                    )),
                    stored: true, clientId: nil, messageId: row.id
                )
            }
        let byClientId = Dictionary(outbox.map { ($0.clientId, $0) }, uniquingKeysWith: { first, _ in first })
        let entries = merge.keep.compactMap { byClientId[$0] }.map { entry in
            let local = entry.entry
            return OwnPendingBubble(
                id: entry.clientId, text: entry.text, at: WireTimestamps.parse(entry.createdAt),
                state: MessageDelivery.deliveryState(MessageDelivery.StateInput(
                    local: local.state, stored: entry.messageId != nil, deliveredAt: nil, consumedAt: nil, echoed: false
                )),
                stored: entry.messageId != nil, clientId: entry.clientId, messageId: entry.messageId
            )
        }
        return (pending + entries, merge.drop)
    }
}
