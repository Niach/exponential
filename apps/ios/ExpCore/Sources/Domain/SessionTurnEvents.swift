import Foundation

/// EXP-1245: the facts the OWNER's thread of turns reads off the relay feed
/// they already hold (`sessionTurns`): their messages (`userMessage` rows,
/// main agent only) and the turn edges. The model keeps the turn as a
/// latest-wins SLOT, not as rows, so this records every edge it OBSERVES into
/// a per-session log that outlives the view: a `started` with its
/// `startedAt`, an `ended` only when it is seen to follow a start (an
/// `ended` slot found on arrival has no time to give). A message is stamped
/// when it ARRIVES live (a lone new row), never a history replay's bulk (its
/// times are unknown, so those rows stay out of the turns). The run's own
/// start opens the first turn (its prompt is the issue or the composer's
/// text, no bubble). Mirrors web `lib/session-turn-events.ts` (same test
/// names).
public final class SessionTurnLog: @unchecked Sendable {
    /// A lone new row (or two: an echo and its twin) is a live arrival.
    public static let liveArrivalMaxRows = 2

    /// Recorded edges, in observation order.
    public private(set) var edges: [SessionTurnEvent] = []
    /// Feed row id → the time (ms) it is placed at.
    public private(set) var messageAt: [Int: Double] = [:]
    private var seen = Set<Int>()
    private var primed = false
    private var lastState: AgentTurnState?
    private var lastStart: Double?

    public init() {}

    /// Fold the turn slot's current value into the log.
    public func recordTurnSlot(_ state: AgentTurnState, startedAt: Double?, now: Double) {
        let start = startedAt.flatMap { $0.isFinite ? $0 : nil }
        if state == .started {
            let at = start ?? now
            if lastState != .started || (start != nil && start != lastStart) {
                edges.append(.turn(started: true, at: at))
            }
            lastStart = start ?? lastStart ?? at
        } else if lastState == .started {
            edges.append(.turn(started: false, at: max(now, lastStart ?? now)))
        }
        lastState = state
    }

    /// One feed row as the log reads it.
    public struct Row: Equatable, Sendable {
        public let id: Int
        public let isUserMessage: Bool
        public let text: String
        public let subagentId: String?

        public init(id: Int, isUserMessage: Bool, text: String = "", subagentId: String? = nil) {
            self.id = id
            self.isUserMessage = isUserMessage
            self.text = text
            self.subagentId = subagentId
        }
    }

    /// The feed's items as rows.
    public static func rows(_ feed: [AgentFeedItem]) -> [Row] {
        feed.map { item in
            if case let .userMessage(id, text, subagentId) = item {
                return Row(id: id, isUserMessage: true, text: text, subagentId: subagentId)
            }
            return Row(id: item.id, isUserMessage: false)
        }
    }

    /// Fold the feed's rows into the log: place every main-agent message.
    public func recordFeedMessages(_ feed: [Row], now: Double) {
        let fresh = feed.filter { !seen.contains($0.id) }
        let live = primed && fresh.count <= Self.liveArrivalMaxRows
        for row in fresh {
            seen.insert(row.id)
            guard row.isUserMessage, row.subagentId == nil else { continue }
            if live { messageAt[row.id] = now }
        }
        primed = true
    }

    /// The events `sessionTurns` walks: the run's start, the placed messages
    /// (still in the feed) and the recorded edges. Empty when nothing was
    /// observed beyond the start (the single-row thread). Images resolve to
    /// `/api/attachments/{id}` (the steer message's trailing embeds).
    public func turnEvents(_ feed: [Row], runStartedAt: Double?) -> [SessionTurnEvent] {
        var messages: [SessionTurnEvent] = []
        for row in feed {
            guard row.isUserMessage, let at = messageAt[row.id], !row.text.isEmpty else { continue }
            let parsed = SteerImageMessage.parse(row.text)
            if parsed.text.isEmpty && parsed.attachmentIds.isEmpty && parsed.files.isEmpty { continue }
            messages.append(.userMessage(
                at: at,
                text: parsed.text,
                images: parsed.attachmentIds.map { "/api/attachments/\($0)" },
                files: parsed.files
            ))
        }
        if messages.isEmpty && edges.isEmpty { return [] }
        let first: [SessionTurnEvent] = runStartedAt.flatMap {
            $0.isFinite ? [.turn(started: true, at: $0)] : nil
        } ?? []
        return first + messages + edges
    }

    // MARK: - One log per run, outliving the view

    private static let lock = NSLock()
    nonisolated(unsafe) private static var logs: [String: SessionTurnLog] = [:]

    public static func log(for sessionId: String) -> SessionTurnLog {
        lock.lock()
        defer { lock.unlock() }
        if let log = logs[sessionId] { return log }
        let log = SessionTurnLog()
        logs[sessionId] = log
        return log
    }
}

/// Whether the FIRST turn's end is a real observation (web
/// `firstTurnEndKnown`, M6 ×4). Its start is the run's own (synthetic), so
/// its end is known only when the view watched it run: the first event after
/// the run's start is an observed `started` edge. Mounting after it ended
/// leaves only the next message to close it, and that time includes the
/// idle gap.
public func firstTurnEndKnown(_ events: [SessionTurnEvent], runStartedAt: Double?) -> Bool {
    guard let first = events.first, case let .turn(started, at) = first, started,
          let start = runStartedAt, at == start
    else { return true }
    let next = events.dropFirst().enumerated()
        .filter { $0.element.at.isFinite }
        .min { a, b in
            a.element.at != b.element.at ? a.element.at < b.element.at : a.offset < b.offset
        }?.element
    guard let next else { return true }
    if case .turn(true, _) = next { return true }
    return false
}
