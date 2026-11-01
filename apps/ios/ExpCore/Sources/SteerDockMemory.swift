import Foundation

/// #EXP-1538 LOCAL FIRST (fixtures/dock.json): the stream's latest-wins slots
/// THE Dock draws from — THE Queue's task list + background tasks, the relay
/// queue, the compaction strip and the rate-limit window. A session view
/// draws them in its first frame from the last-known value while the stream
/// connects, reconnects or is parked, until the live replay restates them in
/// one commit.
public struct SteerDockSlots: Equatable, Sendable {
    public var taskList: [AgentTaskListEntry]
    public var backgroundTasks: [AgentBackgroundTask]
    /// EXP-1324: the client's first sight per background-task id (epoch ms),
    /// so a task the wire gave no `startedAt` keeps counting from it.
    public var backgroundFirstSeen: [String: Int64]
    public var queued: [QueuedMessage]
    public var compacting: AgentCompaction?
    /// When the open compaction started (epoch ms): a seeded strip keeps its
    /// original backstop instead of a fresh full window.
    public var compactingSince: Double?
    public var rateLimit: AgentSessionRateLimit?

    public init(
        taskList: [AgentTaskListEntry] = [],
        backgroundTasks: [AgentBackgroundTask] = [],
        backgroundFirstSeen: [String: Int64] = [:],
        queued: [QueuedMessage] = [],
        compacting: AgentCompaction? = nil,
        compactingSince: Double? = nil,
        rateLimit: AgentSessionRateLimit? = nil
    ) {
        self.taskList = taskList
        self.backgroundTasks = backgroundTasks
        self.backgroundFirstSeen = backgroundFirstSeen
        self.queued = queued
        self.compacting = compacting
        self.compactingSince = compactingSince
        self.rateLimit = rateLimit
    }

    /// Nothing to draw: not worth remembering.
    public var isEmpty: Bool {
        taskList.isEmpty && backgroundTasks.isEmpty && queued.isEmpty
            && compacting == nil && rateLimit == nil
    }
}

/// #EXP-1538: the per-session LAST-KNOWN Dock slots that outlive the socket
/// owner (`SteerSessionStore` retires a detached model past its cap; the next
/// open builds a fresh one that would otherwise draw nothing until its join
/// replay lands). Keyed by an opaque session key (account + session). An
/// ended session is forgotten (it shows none of them), and so is everything
/// on sign-out. Bounded: the oldest remembered session goes first.
public struct SteerDockMemory: Sendable {
    public static let defaultCap = 64

    private let cap: Int
    private var slots: [String: SteerDockSlots] = [:]
    /// Oldest first.
    private var order: [String] = []

    public init(cap: Int = SteerDockMemory.defaultCap) {
        self.cap = max(1, cap)
    }

    public var count: Int { slots.count }

    /// The last-known slots for a session; nil = nothing known.
    public func lastKnown(_ key: String) -> SteerDockSlots? {
        slots[key]
    }

    /// Remember a session's slots as its socket owner goes away. An empty
    /// snapshot forgets: the latest value IS nothing.
    public mutating func remember(_ key: String, _ value: SteerDockSlots) {
        guard !value.isEmpty else {
            forget(key)
            return
        }
        order.removeAll { $0 == key }
        order.append(key)
        slots[key] = value
        while order.count > cap {
            slots[order.removeFirst()] = nil
        }
    }

    /// The session is over (or was claimed again): none of its slots stand.
    public mutating func forget(_ key: String) {
        slots[key] = nil
        order.removeAll { $0 == key }
    }

    /// Sign-out / account switch.
    public mutating func forgetAll() {
        slots = [:]
        order = []
    }
}
