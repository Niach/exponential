import Foundation
import Observation

/// #PERF-5: a main-thread memo over a synced-store lookup that never waits
/// for the database. The chat transcript's layout used to `pool.read` its
/// chips synchronously, and with every reader of the pool busy that froze the
/// app for 11 s (GlitchTip IOS-1). `peek` answers from memory only: a miss is
/// nil, a stale entry its last value, and either loads off main; an answer
/// that differs bumps `StoreMemoLoads.tick`, which `peek` reads, so the view
/// that asked renders again.
@MainActor
public final class StoreMemo<Key: Hashable & Sendable, Value: Equatable & Sendable> {
    private var entries: [Key: (value: Value?, at: Date)] = [:]
    private var loading: [Key: Task<Void, Never>] = [:]
    private let ttl: TimeInterval
    private let capacity: Int
    private let loads: StoreMemoLoads
    private let now: () -> Date

    public init(ttl: TimeInterval = 5, capacity: Int = 512, loads: StoreMemoLoads? = nil, now: @escaping () -> Date = Date.init) {
        self.ttl = ttl
        self.capacity = capacity
        self.loads = loads ?? .shared
        self.now = now
    }

    /// The last answer for `key`, never touching the store (nil = none yet,
    /// or none); a miss or a stale entry runs `load` off main.
    public func peek(_ key: Key, load: @escaping @Sendable () async -> Value?) -> Value? {
        _ = loads.tick
        let hit = entries[key]
        if hit.map({ now().timeIntervalSince($0.at) >= ttl }) ?? true { refresh(key, load) }
        return hit?.value ?? nil
    }

    private func refresh(_ key: Key, _ load: @escaping @Sendable () async -> Value?) {
        guard loading[key] == nil else { return }
        loading[key] = Task { [weak self] in
            let value = await load()
            guard let self else { return }
            self.loading[key] = nil
            let changed = (self.entries[key]?.value ?? nil) != value
            self.store(key, value)
            if changed { self.loads.bump() }
        }
    }

    private func store(_ key: Key, _ value: Value?) {
        if entries.count >= capacity, entries[key] == nil { entries.removeAll(keepingCapacity: true) }
        entries[key] = (value, now())
    }

    /// Tests: every load under way, landed.
    func settle() async {
        while let next = loading.values.first { await next.value }
        await Task.yield()
    }
}

/// #PERF-5: one tick per main-queue turn in which a `StoreMemo` answer
/// changed; the views that peeked read it, so they render the answer, and
/// the editors (which decorate outside SwiftUI) hear `didLoad`.
@MainActor
@Observable
public final class StoreMemoLoads {
    public static let shared = StoreMemoLoads()
    public static let didLoad = Notification.Name("StoreMemoLoads.didLoad")

    public private(set) var tick = 0
    @ObservationIgnored private var bumped = false

    public init() {}

    func bump() {
        guard !bumped else { return }
        bumped = true
        tick &+= 1
        Task { @MainActor [weak self] in
            guard let self else { return }
            self.bumped = false
            NotificationCenter.default.post(name: Self.didLoad, object: self)
        }
    }
}
