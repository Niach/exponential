import ActivityKit
import ExpCore
import ExpPush
import Foundation
import os

private let logger = Logger(subsystem: "at.exponential", category: "LiveActivity")

/// #EXP-1639 (live-activity.json): the app's half of a session's Live
/// Activity. The phone starts nothing itself: the server pushes it through
/// push-relay (push-to-start), so the app only hands over the tokens
/// ActivityKit mints.
///  - ActivityKit's push-to-start token goes to EVERY relay install
///    (`PUT /installs/:id/activity-start-token`), on launch and whenever a
///    push registration lands.
///  - Each started activity's own push token registers with the install
///    holding its account's capability (`POST /installs/:id/activities` →
///    `activityId`), then the account's server learns it
///    (`pushTokens.activity {token, sessionId, activityId}`); the session and
///    the account come from the opened content (`sessionId`, `userId`).
/// Best effort: what fails stays pending until the next registration.
final class LiveActivityCoordinator: @unchecked Sendable {
    private let registrar: PushRelayRegistrar
    private let pushTokensApi: PushTokensApi
    private let auth: AuthRepository
    private let pushKeys: PushKeyStore
    private let relayClient = PushRelayClient()

    private struct Pending: Equatable {
        var token: String
        var sealed: String
    }

    private let lock = NSLock()
    private var fcmToken: String?
    private var startToken: String?
    /// `relay installId token` the start token was PUT with.
    private var startTokenSent: Set<String> = []
    /// Activity id → its latest push token, not yet handed over.
    private var pending: [String: Pending] = [:]
    /// Activity id → the push token the server has.
    private var registered: [String: String] = [:]
    private var observed: Set<String> = []
    private var tasks: [Task<Void, Never>] = []

    init(registrar: PushRelayRegistrar, pushTokensApi: PushTokensApi, auth: AuthRepository, pushKeys: PushKeyStore) {
        self.registrar = registrar
        self.pushTokensApi = pushTokensApi
        self.auth = auth
        self.pushKeys = pushKeys
    }

    func start() {
        guard lock.withLock({ tasks.isEmpty }) else { return }
        let startTokens = Task { [weak self] in
            for await data in Activity<SessionActivityAttributes>.pushToStartTokenUpdates {
                guard let self else { return }
                self.lock.withLock { self.startToken = Self.hex(data) }
                await self.syncStartToken()
            }
        }
        let activities = Task { [weak self] in
            for await activity in Activity<SessionActivityAttributes>.activityUpdates {
                self?.observe(activity)
            }
        }
        lock.withLock { tasks = [startTokens, activities] }
        for activity in Activity<SessionActivityAttributes>.activities { observe(activity) }
    }

    /// A push registration landed (a relay install created or rotated, an FCM
    /// token known): resend the start token and what is pending.
    func registrationLanded(fcmToken: String) {
        let changed = lock.withLock { () -> Bool in
            let changed = self.fcmToken != fcmToken
            self.fcmToken = fcmToken
            if changed {
                // The server keys follows by the FCM row: a new row needs every
                // activity again.
                for (id, token) in registered {
                    if let sealed = Self.sealed(of: id) { pending[id] = Pending(token: token, sealed: sealed) }
                }
                registered.removeAll()
            }
            return changed
        }
        Task {
            await syncStartToken()
            await flushPending()
            if changed { logger.info("Live Activity tokens resent for a new registration") }
        }
    }

    // MARK: - Private

    /// `Activity` is not Sendable: each task re-reads it by id.
    private func observe(_ activity: Activity<SessionActivityAttributes>) {
        observe(id: activity.id)
    }

    private static func activity(_ id: String) -> Activity<SessionActivityAttributes>? {
        Activity<SessionActivityAttributes>.activities.first { $0.id == id }
    }

    private func observe(id: String) {
        let fresh = lock.withLock { observed.insert(id).inserted }
        guard fresh else { return }
        let task = Task { [weak self] in
            guard let tokens = Self.activity(id)?.pushTokenUpdates else { return }
            for await data in tokens {
                guard let self else { return }
                let token = Self.hex(data)
                guard let sealed = Self.sealed(of: id) else { continue }
                let known = self.lock.withLock { () -> Bool in
                    if self.registered[id] == token { return true }
                    self.pending[id] = Pending(token: token, sealed: sealed)
                    return false
                }
                if !known { await self.flushPending() }
            }
            self?.forget(id)
        }
        let ended = Task { [weak self] in
            guard let states = Self.activity(id)?.activityStateUpdates else { return }
            for await state in states where state == .ended || state == .dismissed {
                self?.forget(id)
                return
            }
        }
        lock.withLock { tasks += [task, ended] }
    }

    private func forget(_ id: String) {
        lock.withLock {
            pending[id] = nil
            registered[id] = nil
        }
    }

    private func syncStartToken() async {
        guard let token = lock.withLock({ startToken }) else { return }
        for (relay, install) in await registrar.installs() {
            let key = "\(relay) \(install.installId) \(token)"
            guard lock.withLock({ !startTokenSent.contains(key) }) else { continue }
            do {
                try await relayClient.setActivityStartToken(relay: relay, install: install, token: token)
                lock.withLock { _ = startTokenSent.insert(key) }
                logger.info("Live Activity start token registered")
            } catch {
                logger.error("Live Activity start token failed: \(error.localizedDescription)")
            }
        }
    }

    private func flushPending() async {
        let (work, fcm) = lock.withLock { (pending, fcmToken) }
        guard let fcm else { return }
        for (id, item) in work {
            let display = LiveActivityDisplay.open(sealed: item.sealed, keys: pushKeys.loadAll())
            guard let sessionId = display.sessionId, let userId = display.userId else {
                // Not opened here (no key yet): stays pending.
                continue
            }
            let accounts = auth.accounts.filter { $0.userId == userId && $0.token != nil }.map(\.id)
            var target: (accountId: String, relay: String, install: PushRelayInstall)?
            for accountId in accounts {
                if let found = await registrar.install(forAnyOf: [accountId]) {
                    target = (accountId, found.relay, found.install)
                    break
                }
            }
            guard let target else { continue }
            do {
                let activityId = try await relayClient
                    .registerActivity(relay: target.relay, install: target.install, token: item.token)
                try await pushTokensApi.activity(
                    accountId: target.accountId, token: fcm, sessionId: sessionId, activityId: activityId
                )
                lock.withLock {
                    if pending[id] == item { pending[id] = nil }
                    registered[id] = item.token
                }
                logger.info("Live Activity registered")
            } catch {
                logger.error("Live Activity registration failed: \(error.localizedDescription)")
            }
        }
    }

    private static func sealed(of id: String) -> String? {
        Activity<SessionActivityAttributes>.activities.first { $0.id == id }?.content.state.s
    }

    private static func hex(_ data: Data) -> String {
        data.map { String(format: "%02x", $0) }.joined()
    }
}
