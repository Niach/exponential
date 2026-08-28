import Foundation

/// EXP-656: when each account's `devices` shape last completed a poll.
///
/// Presence ("that machine is online") is derived client-side from
/// `last_seen_at`, so it is only as current as our own cursor: after a
/// suspension the rows still carry the pre-sleep stamp and every session on
/// the phone renders "Paused" until the shape catches up. The rule the
/// surfaces apply is `DeviceFreshness.isTrustworthy` — a devices cursor we
/// have not refreshed within the contract window can only produce a false
/// OFFLINE (last_seen_at only moves forward), so an unrefreshed cursor
/// renders presence as UNKNOWN instead.
///
/// Stamped by `ShapeClient.pollOnce` on a successful devices response rather
/// than observed off GRDB: a poll that answers a bare `up-to-date` writes no
/// rows, and that is exactly the answer a healthy catch-up gets.
///
/// `@unchecked Sendable` for the usual reason: every access goes through the
/// lock, and the shape loops that stamp it run off the main actor while the
/// view models that read it are main-actor isolated.
public final class SyncFreshness: @unchecked Sendable {
    public static let shared = SyncFreshness()

    /// The shape name that carries device presence — `ShapeClient` compares
    /// its own against this, so the stamp can never drift from the pipeline's
    /// spelling in SyncManager.
    public static let devicesShapeName = "devices"

    private let lock = NSLock()
    private var polledAt: [String: Date] = [:]
    private var subscribers: [UUID: AsyncStream<String>.Continuation] = [:]

    /// A devices poll for this account came back. Wakes every listener with
    /// the account id so the presence-rendering models re-derive.
    public func recordDevicesPoll(accountId: String, at: Date = Date()) {
        let listeners = lock.withLock { () -> [AsyncStream<String>.Continuation] in
            polledAt[accountId] = at
            return Array(subscribers.values)
        }
        for listener in listeners { listener.yield(accountId) }
    }

    /// When this account's devices shape last answered; nil = never polled on
    /// this run, which is as untrustworthy as a stale cursor.
    public func devicesPolledAt(accountId: String) -> Date? {
        lock.withLock { polledAt[accountId] }
    }

    /// Account ids whose devices cursor just advanced. Also the missing
    /// foreground re-derivation hook: the resume's catch-up poll lands here,
    /// so a viewer repaints the moment its knowledge is current again instead
    /// of waiting out a 30s liveness tick.
    public func updates() -> AsyncStream<String> {
        AsyncStream { continuation in
            let id = UUID()
            lock.withLock { subscribers[id] = continuation }
            continuation.onTermination = { [weak self] _ in
                guard let self else { return }
                self.lock.withLock { _ = self.subscribers.removeValue(forKey: id) }
            }
        }
    }

    /// Test seam: forget every stamp (the singleton outlives one test case).
    func reset() {
        lock.withLock { polledAt = [:] }
    }
}
