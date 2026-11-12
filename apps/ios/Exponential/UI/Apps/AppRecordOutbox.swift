import ExpCore
import ExpUI
import Foundation
import GRDB
import SwiftUI
#if canImport(ExponentialEngine)
import ExponentialEngine
#endif

/// A host call's answer from the outbox: the server's or the entry's, as JSON.
enum AppRecordOutboxAnswer: Sendable {
    /// The server took it.
    case done(String)
    /// The server refused it (the entry is gone): the call fails. `invalid`
    /// = invalid input (BAD_REQUEST): a record form shows it inline.
    case refused(message: String, invalid: Bool)
    /// Offline, behind others, a retry or a drop: the answer from the entry.
    case queued(String)

    static func value(_ json: String) -> Any? {
        let value = try? JSONSerialization.jsonObject(with: Data(json.utf8), options: [.fragmentsAllowed])
        return value is NSNull ? nil : value
    }
}

/// VAPP-235 (app-records.json `outbox`): the running app's header glyph
/// while it has writes waiting to sync (the editor's and the run view's):
/// the muted `ui-offline` glyph, labelled with the count; none = nothing.
struct AppOutboxPendingGlyph: View {
    let appId: String
    @Environment(\.accountId) private var accountId
    @Environment(AppDependencies.self) private var deps
    @State private var label: String?

    var body: some View {
        // A ZStack, not a Group: an empty Group hands its modifiers to no
        // child, so the task below would never run.
        ZStack {
            if let label {
                AppIcon(AppIcons.uiOffline, size: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .help(label)
                    .accessibilityLabel(label)
                    .accessibilityIdentifier("app-outbox-pending")
            }
        }
        .task(id: "\(accountId)/\(appId)") {
            label = nil
            await AppRecordOutbox.pendingLabels(accountId: accountId, appId: appId, db: deps.db) { label = $0 }
        }
    }
}

#if canImport(ExponentialEngine)

/// VAPP-235 (eApps O1, app-records.json `outbox`, docs/spec/apps.md §14) /
/// VAPP-272: the OFFLINE OUTBOX of the app hosts' record writes, on the
/// Engine. Each account has one `SyncedRecordStore`: the queue, the flusher,
/// the waiters and the prune live there (`outbox.json` `syncedCases`); this
/// file fills in its providers (the account's `app_record_outbox` table, tRPC)
/// and says when to flush: on every enqueue, when the network comes back
/// (`NetworkPathWatcher`), at launch, and on a timer while entries wait.
@MainActor
final class AppRecordOutbox {
    private let db: DatabaseManager
    private let api: AppsApi
    private let auth: AuthRepository
    /// Each account's store over the pool it opened on (a wiped database = a new pool = a new queue).
    private var stores: [String: (pool: DatabasePool, store: SyncedRecordStore)] = [:]
    /// A store being opened (`store(_:)`), so concurrent callers share one.
    private var opening: [String: (pool: DatabasePool, task: Task<SyncedRecordStore, Error>)] = [:]
    /// The account's waiting retry (`retryWhilePending`).
    private var retrying: [String: Task<Void, Never>] = [:]
    private static let retryInterval: Duration = .seconds(30)
    /// The Engine's calls block on the network: never on the main thread or
    /// Swift's cooperative pool.
    nonisolated private static let replays = DispatchQueue(label: "at.exponential.app-records", attributes: .concurrent)

    nonisolated init(db: DatabaseManager, api: AppsApi, auth: AuthRepository) {
        self.db = db
        self.api = api
        self.auth = auth
    }

    /// #PERF-5: the Engine reads the log as it opens a store (`entriesJson`,
    /// a blocking pool read): that runs on `replays`, never on main.
    private func store(_ accountId: String) async throws -> SyncedRecordStore {
        let pool = try db.pool(forAccountId: accountId)
        if let open = stores[accountId], open.pool === pool { return open.store }
        if let pending = opening[accountId], pending.pool === pool { return try await pending.task.value }
        let api = api
        let task = Task { () throws -> SyncedRecordStore in
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<SyncedRecordStore, Error>) in
                Self.replays.async {
                    continuation.resume(with: Result {
                        try SyncedRecordStore(
                            log: AppOutboxLog(pool: pool),
                            transport: AppOutboxTransport(accountId: accountId, api: api, pool: pool),
                            listener: AppOutboxToasts(),
                            clock: nil
                        )
                    })
                }
            }
        }
        opening[accountId] = (pool, task)
        defer { if opening[accountId]?.pool === pool { opening[accountId] = nil } }
        let store = try await task.value
        stores[accountId] = (pool, store)
        return store
    }

    /// Queue one host call (`records_upsert` | `records_delete` |
    /// `records_batch`): the only pending entry awaits its first replay,
    /// otherwise it answers from the entry at once.
    func write(accountId: String, appId: String, function: String, args: [String: CardJSON]) async throws -> AppRecordOutboxAnswer {
        let store = try await store(accountId)
        let json = CardJSON.object(args).compactString
        let submitted = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Submitted, Error>) in
            Self.replays.async {
                continuation.resume(with: Result { try store.submitCall(app: appId, function: function, argsJson: json, online: true) })
            }
        }
        switch submitted {
        case let .done(answerJson): return .done(answerJson)
        case let .refused(message, invalid): return .refused(message: message ?? "", invalid: invalid)
        case let .queued(answerJson):
            flush(accountId: accountId)
            return .queued(answerJson)
        }
    }

    /// Flush every signed-in account (launch, the network coming back).
    func flushAll() {
        for accountId in auth.authenticatedAccountIds.sorted() {
            flush(accountId: accountId)
        }
    }

    /// Run the account's flusher (one at a time: the Engine turns a call while
    /// it runs into another pass).
    func flush(accountId: String) {
        Task { @MainActor in
            guard let store = try? await self.store(accountId) else { return }
            Self.replays.async {
                store.flush()
                Task { @MainActor in self.retryWhilePending(accountId: accountId) }
            }
        }
    }

    /// The synced rows of `appId` a binding read: acked entries they reflect
    /// (or past the grace) leave.
    func reflect(accountId: String, appId: String, rows: [AppRecords.Row]) {
        let json = CardJSON.array(rows.map(Self.rowJSON)).compactString
        Task { @MainActor in
            guard let store = try? await self.store(accountId) else { return }
            Self.replays.async { try? store.setRows(app: appId, rowsJson: json) }
        }
    }

    private nonisolated static func rowJSON(_ row: AppRecords.Row) -> CardJSON {
        var object: [String: CardJSON] = [
            "id": .string(row.id), "kind": .string(row.kind), "data": .object(row.data),
            "createdAt": .string(row.createdAt), "updatedAt": .string(row.updatedAt),
        ]
        if let createdBy = row.createdBy { object["createdBy"] = .string(createdBy) }
        if let authorId = row.authorId { object["authorId"] = .string(authorId) }
        return .object(object)
    }

    /// A server that goes down while the network stays up raises no network
    /// edge to replay on: entries a drain left pending try again on a timer
    /// (one per account), instead of waiting for a relaunch.
    private func retryWhilePending(accountId: String) {
        guard retrying[accountId] == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        retrying[accountId] = Task { @MainActor in
            let pending = (try? await pool.read { db in
                try AppRecordOutboxEntity.filter(Column("state") == "pending").fetchCount(db)
            }) ?? 0
            if pending > 0 { try? await Task.sleep(for: Self.retryInterval) }
            self.retrying[accountId] = nil
            if pending > 0, !Task.isCancelled, self.auth.authenticatedAccountIds.contains(accountId) {
                self.flush(accountId: accountId)
            }
        }
    }

    /// The running app's header glyph: `outboxPendingLabel` of the account's
    /// entries for the app, on every outbox change (nil = no glyph).
    static func pendingLabels(accountId: String, appId: String, db: DatabaseManager, _ update: @escaping @MainActor (String?) -> Void) async {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try AppRecordOutboxEntity.filter(Column("app_id") == appId).order(Column("seq")).fetchAll(db)
        }
        do {
            for try await rows in observation.values(in: pool) {
                update(AppRecords.outboxPendingLabel(rows.compactMap(\.entry), appId: appId))
            }
        } catch {}
    }
}

/// The account's `app_record_outbox` table.
private final class AppOutboxLog: OutboxLog, @unchecked Sendable {
    private let pool: DatabasePool

    init(pool: DatabasePool) {
        self.pool = pool
    }

    private func failing<T>(_ body: () throws -> T) throws -> T {
        do {
            return try body()
        } catch {
            throw EngineError.Failure(code: "INTERNAL", reason: "\(error)")
        }
    }

    func entriesJson() throws -> String {
        try failing {
            let rows = try pool.read { db in try AppRecordOutboxEntity.order(Column("seq")).fetchAll(db) }
            // An unreadable row can never replay: it leaves.
            let unreadable = rows.filter { $0.entry == nil }.map(\.writeId)
            if !unreadable.isEmpty { try remove(writeIds: unreadable) }
            return CardJSON.array(rows.compactMap { $0.entry?.json }).compactString
        }
    }

    func push(entryJson: String) throws {
        try failing {
            let parsed = try JSONSerialization.jsonObject(with: Data(entryJson.utf8))
            guard case let .object(object)? = CardJSON(any: parsed),
                  case let .string(writeId)? = object["writeId"], case let .string(appId)? = object["appId"],
                  case let .string(call)? = object["call"], case let .string(at)? = object["at"], let writes = object["writes"]
            else { throw AppHostMcp.CallError(message: "unreadable entry") }
            try pool.write { db in
                let seq = try Int64.fetchOne(db, sql: #"SELECT COALESCE(MAX("seq"), 0) FROM "app_record_outbox""#) ?? 0
                try AppRecordOutboxEntity(writeId: writeId, seq: seq + 1, appId: appId, call: call, writes: writes.compactString, at: at).insert(db)
            }
        }
    }

    func ack(writeId: String, at: String) throws {
        try failing {
            try pool.write { db in
                try db.execute(
                    sql: #"UPDATE "app_record_outbox" SET "state" = 'acked', "acked_at" = ? WHERE "write_id" = ?"#,
                    arguments: [at, writeId]
                )
            }
        }
    }

    func remove(writeIds: [String]) throws {
        try failing {
            try pool.write { db in
                for writeId in writeIds {
                    try db.execute(sql: #"DELETE FROM "app_record_outbox" WHERE "write_id" = ?"#, arguments: [writeId])
                }
            }
        }
    }
}

/// tRPC out, as the account and the entry's app. `push` blocks: the Engine
/// runs it on `AppRecordOutbox.replays`.
private final class AppOutboxTransport: SyncTransport, @unchecked Sendable {
    private let accountId: String
    private let api: AppsApi
    private let pool: DatabasePool

    init(accountId: String, api: AppsApi, pool: DatabasePool) {
        self.accountId = accountId
        self.api = api
        self.pool = pool
    }

    func push(app: String, call: String, inputJson: String) -> String {
        let parsed = try? JSONSerialization.jsonObject(with: Data(inputJson.utf8))
        guard case let .object(input)? = CardJSON(any: parsed), let call = AppRecords.OutboxCall(rawValue: call) else {
            return #"{"code":"BAD_REQUEST"}"#
        }
        let done = DispatchSemaphore(value: 0)
        let reply = AppOutboxReply()
        let (accountId, api, pool) = (accountId, api, pool)
        Task.detached {
            reply.text = await Self.reply(accountId: accountId, api: api, pool: pool, app: app, call: call, input: input)
            done.signal()
        }
        done.wait()
        return reply.text
    }

    private static func reply(
        accountId: String, api: AppsApi, pool: DatabasePool, app: String, call: AppRecords.OutboxCall, input: [String: CardJSON]
    ) async -> String {
        // VAPP-231 (via-app.json `claim`): the synced row at send time.
        let claim = await AppHostMcp.hostClaim(appId: app, pool: pool)
        do {
            let replayed = try await api.recordsReplay(accountId: accountId, appId: app, call: call, input: input, viaApp: claim)
            let answer: CardJSON
            switch replayed {
            case let .upserted(result): answer = .object(["id": .string(result.id)])
            case .deleted: answer = .null
            case let .batched(result):
                answer = .object(["ids": .array(result.ids.map(CardJSON.string)), "eventIds": .array((result.eventIds ?? []).map(CardJSON.string))])
            }
            return CardJSON.object(["ok": .bool(true), "answer": answer]).compactString
        } catch {
            guard case let .code(code) = AppRecords.outboxResult(error) else { return #"{"transport":true}"# }
            return CardJSON.object([
                "code": .string(code), "message": .string(error.trpcUserMessage), "invalid": .bool(error.trpcErrorCode == "BAD_REQUEST"),
            ]).compactString
        }
    }
}

/// One replay's reply across the semaphore.
private final class AppOutboxReply: @unchecked Sendable {
    var text = #"{"transport":true}"#
}

/// A later replay's refusal: the failed-action toast.
private final class AppOutboxToasts: SyncedListener, @unchecked Sendable {
    func changed() {}

    func refused(entryJson: String, verb: String, message: String?) {
        Task { @MainActor in
            Toaster.shared.error(FeedbackStates.Errors.title(verb: verb, thing: "the record"), description: message ?? "")
        }
    }
}

#else

/// Without the Engine's xcframework (a bare `tuist generate`): the app builds,
/// and an app's writes fail. Every shipped build has it (build-ios.yml).
@MainActor
final class AppRecordOutbox {
    nonisolated init(db: DatabaseManager, api: AppsApi, auth: AuthRepository) {}

    func write(accountId: String, appId: String, function: String, args: [String: CardJSON]) async throws -> AppRecordOutboxAnswer {
        throw AppHostMcp.CallError(message: "Build the Engine: apps/desktop/crates/eengine-ffi/build-ios.sh")
    }

    func flushAll() {}

    func reflect(accountId: String, appId: String, rows: [AppRecords.Row]) {}

    static func pendingLabels(accountId: String, appId: String, db: DatabaseManager, _ update: @escaping @MainActor (String?) -> Void) async {}
}

#endif
