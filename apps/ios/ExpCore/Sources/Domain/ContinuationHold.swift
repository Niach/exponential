import Foundation

/// EXP-935: a resume or an account switch ENDS the run it continues, on
/// purpose — so between the command going out and the successor row syncing
/// in, the screen showing that run must HOLD instead of popping back to the
/// list. This is that hold, as a pure state machine: `sending` (the command is
/// on the wire) → `sent` (delivered, watching for the row) → `landed` (the
/// successor's id, which the screen swaps in place) or `expired` (the
/// watcher's deadline passed with no row, so the screen falls back to its
/// ordinary ended behaviour).
///
/// It is deliberately NOT a view flag: the run's own face is unmounted while
/// the reader is on another face, and a hold bridged through a view
/// preference dies with it (the bug this replaces).
public struct ContinuationHold: Equatable, Sendable {

    public enum State: Equatable, Sendable {
        case idle
        /// The start command is on the wire; nothing is being watched yet.
        case sending
        /// Delivered at this moment — the successor row is expected.
        case sent(at: Date)
        /// The successor synced in.
        case landed(id: String)
        /// The deadline passed (or the server refused the send).
        case expired
    }

    /// How long a continuation may take to surface its row — the watcher's
    /// own deadline (`StartedRunMatch.deadline`), so the hold and the caption
    /// end together.
    public static let deadline: TimeInterval = StartedRunMatch.deadline

    public private(set) var state: State = .idle

    public init() {}

    /// The screen must stay put: a continuation is on its way.
    public var isPending: Bool {
        switch state {
        case .sending, .sent: return true
        case .idle, .landed, .expired: return false
        }
    }

    /// The successor, once it landed.
    public var landedId: String? {
        if case let .landed(id) = state { return id }
        return nil
    }

    public mutating func sending() {
        state = .sending
    }

    /// Delivered — from here the deadline runs.
    public mutating func sent(at moment: Date = Date()) {
        state = .sent(at: moment)
    }

    public mutating func landed(_ id: String) {
        state = .landed(id: id)
    }

    /// The send was refused, or the deadline passed.
    public mutating func expire() {
        guard isPending else { return }
        state = .expired
    }

    /// The wall-clock check: a hold whose deadline passed stops holding even
    /// if nothing else ever fires (no row syncing in means no other edge).
    public mutating func tick(now: Date = Date()) {
        guard case let .sent(at) = state else { return }
        if now.timeIntervalSince(at) >= Self.deadline { state = .expired }
    }

    /// Back to nothing pending — a screen showing another run.
    public mutating func reset() {
        state = .idle
    }
}
