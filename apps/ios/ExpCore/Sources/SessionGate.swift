import Foundation

/// The dead-session gate, keyed PER ACCOUNT. When a server definitively rejects
/// an account's credential — HTTP 401 on a bearer-authed tRPC/shape request, or
/// a `get-session` that answers 2xx with NO user while a token WAS presented
/// (Better Auth's answer for a dead bearer) — the layer that saw it trips the
/// gate for the owning account, and SyncManager's reconcile loop turns that into
/// a LOCAL sign-out of that one account. Every other signed-in server keeps
/// syncing, exactly like the 426 `UpdateGate` it mirrors (REV2-43); the app's
/// nav gate then takes over, because a tokenless account set is what shows
/// LoginView.
///
/// Deliberately NOT `@Observable`: nothing observes the gate itself. The visible
/// effect is the cleared token on `AuthRepository`, which already publishes. So
/// the state is lock-guarded instead of main-actor-mutated, since the trip
/// points are the shape loops and every read happens off the main actor.
///
/// Transport errors, timeouts and 5xx must NEVER reach here — "the server is
/// unreachable" has to stay indistinguishable from offline, or a flaky network
/// would sign people out.
public final class SessionGate: @unchecked Sendable {
    public static let shared = SessionGate()

    private let lock = NSLock()
    private var invalidated: Set<String> = []

    private init() {}

    /// Record that `accountId`'s credential is definitively dead. Idempotent —
    /// 16 shape loops plus any in-flight tRPC calls all race here once a session
    /// dies.
    public func invalidate(accountId: String) {
        guard !accountId.isEmpty else { return }
        lock.withLock { _ = invalidated.insert(accountId) }
    }

    /// Accounts whose credential a server has rejected. Read-only view for
    /// diagnostics and tests; the handler uses `consumeAll()`.
    public var invalidatedAccountIds: Set<String> {
        lock.withLock { invalidated }
    }

    public func isInvalidated(accountId: String) -> Bool {
        lock.withLock { invalidated.contains(accountId) }
    }

    /// Take and clear every pending entry. Consuming (rather than keeping the
    /// entry around) is what lets the user sign straight back INTO the same
    /// account: a sticky entry would clear the fresh token on the next
    /// reconcile tick.
    public func consumeAll() -> Set<String> {
        lock.withLock {
            let pending = invalidated
            invalidated.removeAll()
            return pending
        }
    }

    /// Forget one account's entry — for test isolation, and any future caller
    /// that handles the sign-out itself.
    public func clear(accountId: String) {
        lock.withLock { _ = invalidated.remove(accountId) }
    }
}
