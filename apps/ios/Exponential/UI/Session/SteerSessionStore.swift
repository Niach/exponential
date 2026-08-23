import Foundation

/// EXP-621: steer sessions outlive the screen showing them. The socket owner
/// (AgentSessionModel) used to be view `@State` torn down by `onDisappear`, so
/// every visit paid a ticket mint + dial + full activity replay behind
/// "Connecting…", and anything typed into the composer was gone. The models now
/// live here — app-scoped, keyed by account + session — so popping back to a
/// session shows its feed instantly and its draft is still in the composer.
///
/// Lifetime: a model is created on the first attach and kept while the session
/// is running, whether or not a view is on screen. It is dropped when the
/// session is over and nothing is showing it, and detached-but-live models are
/// capped so a browse through many sessions can't hold an unbounded pile of
/// sockets open.
@MainActor @Observable
final class SteerSessionStore {
    private struct Key: Hashable {
        let accountId: String
        let sessionId: String
    }

    private final class Entry {
        let model: AgentSessionModel
        /// How many views currently have this session on screen (a push over
        /// the top and back is a detach/attach pair, and a swipe-back that
        /// re-attaches before the old view's onDisappear would otherwise reap
        /// a model still in use).
        var attached = 0
        var detachedAt = Date.distantPast

        init(model: AgentSessionModel) {
            self.model = model
        }
    }

    /// How many DETACHED sessions keep their socket. One is the normal case
    /// (the session you just backed out of); the cap only bites when someone
    /// walks through several live sessions in a row, and it evicts the least
    /// recently visited — which is exactly the one whose replay nobody is
    /// waiting on.
    private static let detachedCap = 3

    private var entries: [Key: Entry] = [:]

    /// Constructed alongside the other long-lived services in AppDependencies,
    /// whose init is not main-actor isolated.
    nonisolated init() {}

    /// The model for one session, created on first use. `make` is only called
    /// when there is nothing to reuse — the caller owns the dependencies, so
    /// the store never has to reach back into AppDependencies.
    func attach(
        accountId: String,
        sessionId: String,
        make: () -> AgentSessionModel
    ) -> AgentSessionModel {
        let key = Key(accountId: accountId, sessionId: sessionId)
        if let entry = entries[key] {
            entry.attached += 1
            // A no-op unless something shut this model down (removeAll), in
            // which case it re-arms its observations and redials.
            entry.model.resume()
            // Opening the screen is as good a reason to stop waiting out a
            // reconnect backoff as foregrounding the app: a model parked at the
            // 30s cap would otherwise show a stale feed under "Reconnecting…"
            // for half a minute after the user asked for it.
            entry.model.reconnectNow()
            return entry.model
        }
        let entry = Entry(model: make())
        entry.attached = 1
        entries[key] = entry
        entry.model.start()
        return entry.model
    }

    /// The view showing this session went away. The socket stays up — only a
    /// finished session (or one pushed past the retention cap) is torn down.
    func detach(accountId: String, sessionId: String) {
        let key = Key(accountId: accountId, sessionId: sessionId)
        guard let entry = entries[key] else { return }
        entry.attached = max(0, entry.attached - 1)
        if entry.attached == 0 { entry.detachedAt = Date() }
        reap()
    }

    /// Foreground revival (EXP-243, now app-scoped): a suspension rarely leaves
    /// a socket alive, so every retained session redials on the way back in
    /// instead of waiting for its screen to be opened again.
    func reconnectAll() {
        reap()
        for entry in entries.values {
            entry.model.reconnectNow()
        }
    }

    /// Drop everything (sign-out / account switch): sockets closed, drafts and
    /// feeds gone. Nothing about one account's session may survive into
    /// another's session list.
    func removeAll() {
        for entry in entries.values {
            entry.model.shutdown()
        }
        entries = [:]
    }

    /// Retire finished sessions nobody is looking at, then enforce the
    /// detached cap by least-recently-detached.
    private func reap() {
        let finished = entries.keys.filter { key in
            guard let entry = entries[key] else { return false }
            return entry.attached == 0 && entry.model.isOver
        }
        for key in finished { drop(key) }
        let detached = entries.keys
            .filter { entries[$0]?.attached == 0 }
            .sorted { a, b in
                (entries[a]?.detachedAt ?? .distantPast) < (entries[b]?.detachedAt ?? .distantPast)
            }
        guard detached.count > Self.detachedCap else { return }
        for key in detached.prefix(detached.count - Self.detachedCap) { drop(key) }
    }

    private func drop(_ key: Key) {
        entries[key]?.model.shutdown()
        entries[key] = nil
    }
}
