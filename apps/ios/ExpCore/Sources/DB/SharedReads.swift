import Foundation
import GRDB

/// #ERR-26 (Android #ERR-23 `SharedReads.kt`): a `ValueObservation` over a
/// WHOLE table fetches once per observer and each observer keeps its own
/// copy, re-fetched on EVERY synced write (GRDB fetches even while the
/// consumer still sleeps, so a throttle in the consumer bounds nothing). On a
/// large account (thousands of issues with their descriptions, thousands of
/// sessions with their digests) that is several copies alive at once and a
/// new one per write.
///
/// A `SharedRead` is ONE read per account database: every subscriber gets the
/// same value, the read re-runs on the first change at once and then at most
/// once per `minInterval` (changes landing meanwhile fold into one re-read,
/// so the newest rows always arrive), and the value is dropped `stopAfter`
/// the last subscriber leaves (a returning subscriber never sees a stale
/// table: the read starts again).
@MainActor
public final class SharedRead<Value: Sendable> {
    private let writer: any DatabaseWriter
    private let tables: [String]
    private let minInterval: Duration
    private let stopAfter: Duration
    private let load: @Sendable (any DatabaseWriter) async throws -> Value
    private let same: (@Sendable (Value, Value) -> Bool)?

    private var subscribers: [UUID: AsyncStream<Value>.Continuation] = [:]
    private var latest: Value?
    private var loop: Task<Void, Never>?
    private var stopping: Task<Void, Never>?
    /// Every read so far, whatever the subscribers (the memory budget: one
    /// read per change window, never one per subscriber).
    public private(set) var reads = 0

    /// `load` reads the value off main (`tables` = what a change to
    /// re-reads); it may derive inside, so the derived value is what's shared.
    /// `same` drops a re-read equal to the value held (subscribers keep the
    /// one instance, never an equal second one).
    public nonisolated init(
        writer: any DatabaseWriter,
        tables: [String],
        minInterval: Duration = SharedReads.minInterval,
        stopAfter: Duration = SharedReads.stopAfter,
        same: (@Sendable (Value, Value) -> Bool)? = nil,
        load: @escaping @Sendable (any DatabaseWriter) async throws -> Value
    ) {
        self.writer = writer
        self.tables = tables
        self.minInterval = minInterval
        self.stopAfter = stopAfter
        self.same = same
        self.load = load
    }

    public nonisolated convenience init(
        writer: any DatabaseWriter,
        tables: [String],
        minInterval: Duration = SharedReads.minInterval,
        stopAfter: Duration = SharedReads.stopAfter,
        same: (@Sendable (Value, Value) -> Bool)? = nil,
        fetch: @escaping @Sendable (Database) throws -> Value
    ) {
        self.init(writer: writer, tables: tables, minInterval: minInterval, stopAfter: stopAfter, same: same) {
            try await $0.read(fetch)
        }
    }

    /// The value now (when one landed) and after every re-read; cancelling the
    /// iterating task leaves. A failed read (a closed database) ends it.
    public func values() -> AsyncStream<Value> {
        let id = UUID()
        let (stream, continuation) = AsyncStream<Value>.makeStream(bufferingPolicy: .bufferingNewest(1))
        continuation.onTermination = { [weak self] _ in
            Task { @MainActor in self?.leave(id) }
        }
        subscribers[id] = continuation
        stopping?.cancel()
        stopping = nil
        if let latest { continuation.yield(latest) }
        if loop == nil { start() }
        return stream
    }

    /// Tests: subscribers right now, and whether the read is live.
    public var subscriberCount: Int { subscribers.count }
    public var isLive: Bool { loop != nil }

    private func leave(_ id: UUID) {
        subscribers[id] = nil
        guard subscribers.isEmpty, loop != nil, stopping == nil else { return }
        stopping = Task { [weak self, stopAfter] in
            try? await Task.sleep(for: stopAfter)
            guard !Task.isCancelled else { return }
            self?.stopIfIdle()
        }
    }

    private func stopIfIdle() {
        stopping = nil
        guard subscribers.isEmpty else { return }
        loop?.cancel()
        loop = nil
        latest = nil
    }

    private func start() {
        let writer = writer, tables = tables, load = load, minInterval = minInterval
        loop = Task { [weak self] in
            // Tracking starts BEFORE the first read: a write landing during it
            // re-reads after the interval.
            let (changes, observation) = await Self.changes(writer, tables)
            defer { observation.cancel() }
            // Never holds `self` across a wait: an owner dropped mid-loop ends it.
            @MainActor func step() async throws -> Bool {
                let value = try await load(writer)
                try Task.checkCancellation()
                guard let self else { return false }
                self.deliver(value)
                try await Task.sleep(for: minInterval)
                return true
            }
            do {
                guard try await step() else { return }
                for await _ in changes {
                    guard try await step() else { return }
                }
            } catch is CancellationError {
            } catch {
                self?.fail()
            }
        }
    }

    private func deliver(_ value: Value) {
        reads += 1
        if let same, let latest, same(latest, value) { return }
        latest = value
        for continuation in subscribers.values { continuation.yield(value) }
    }

    private func fail() {
        let ended = subscribers.values
        subscribers = [:]
        loop = nil
        latest = nil
        for continuation in ended { continuation.finish() }
    }

    /// One signal per committed change to `tables`, folded (at most one
    /// pending). Started off main: GRDB registers it on the writer.
    nonisolated private static func changes(
        _ writer: any DatabaseWriter, _ tables: [String]
    ) async -> (AsyncStream<Void>, AnyDatabaseCancellable) {
        await withCheckedContinuation { done in
            DispatchQueue.global(qos: .utility).async {
                let (stream, continuation) = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
                let observation = DatabaseRegionObservation(tracking: tables.map { Table($0) })
                    .start(in: writer, onError: { _ in continuation.finish() }, onChange: { _ in continuation.yield() })
                done.resume(returning: (stream, observation))
            }
        }
    }
}

/// The shared reads of ONE account database (`DatabaseManager.sharedReads`):
/// a read is keyed, so every model asking for the same key shares one value.
@MainActor
public final class SharedReads {
    /// The fastest a whole table is re-read while the sync keeps writing it.
    public nonisolated static let minInterval: Duration = .seconds(1)
    /// A screen change (old screen leaves, new one arrives) keeps the value.
    public nonisolated static let stopAfter: Duration = .seconds(5)

    nonisolated let writer: any DatabaseWriter
    private var reads: [String: AnyObject] = [:]

    public nonisolated init(writer: any DatabaseWriter) {
        self.writer = writer
    }

    /// The read under `key`, made on first ask (`load`/`tables` of the first
    /// asker win: one key = one meaning).
    public func shared<Value: Sendable>(
        _ key: String,
        tables: [String],
        minInterval: Duration = SharedReads.minInterval,
        same: (@Sendable (Value, Value) -> Bool)? = nil,
        load: @escaping @Sendable (any DatabaseWriter) async throws -> Value
    ) -> SharedRead<Value> {
        if let existing = reads[key] as? SharedRead<Value> { return existing }
        let read = SharedRead(writer: writer, tables: tables, minInterval: minInterval, same: same, load: load)
        reads[key] = read
        return read
    }

    /// An equatable read drops a re-read equal to the value held.
    public func shared<Value: Sendable & Equatable>(
        _ key: String,
        tables: [String],
        minInterval: Duration = SharedReads.minInterval,
        load: @escaping @Sendable (any DatabaseWriter) async throws -> Value
    ) -> SharedRead<Value> {
        shared(key, tables: tables, minInterval: minInterval, same: { $0 == $1 }, load: load)
    }

    /// Every issue row, whole (the composer's picker ranks descriptions).
    public var issues: SharedRead<[IssueEntity]> {
        shared("issues", tables: [IssueEntity.databaseTableName]) { writer in
            try await writer.read { try IssueEntity.fetchAll($0) }
        }
    }
}

public extension FetchableRecord where Self: TableRecord {
    /// #ERR-26: every row with every column but `excluded` (decoded as nil):
    /// the readers that only list rows leave their big text out.
    static func all(_ db: Database, except excluded: Set<String>) throws -> QueryInterfaceRequest<Self> {
        let columns = try db.columns(in: databaseTableName).map(\.name).filter { !excluded.contains($0) }
        return all().select(columns.map { Column($0) })
    }
}

public extension IssueEntity {
    /// #ERR-26: every issue without its `description` (4.9 MB on a large
    /// account): the inbox and the home widgets draw identifier and title.
    static func withoutDescription(_ db: Database) throws -> QueryInterfaceRequest<IssueEntity> {
        try all(db, except: ["description"])
    }
}
