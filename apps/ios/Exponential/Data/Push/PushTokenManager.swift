import ExpUI
import ExpCore
import Foundation
import os

private let logger = Logger(subsystem: "at.exponential", category: "PushTokenManager")

/// Keeps the device's FCM token registered for EVERY signed-in account, not
/// just whichever account was active when the Messaging delegate callback
/// fired. Mirrors SyncManager's polling reconcile over `auth.accounts`: a
/// login, account add, or switch that happens after launch gets its
/// registration within one tick instead of waiting for the next cold start.
/// The server keys registrations per (token, user), so registering one
/// account never displaces another's row.
final class PushTokenManager: @unchecked Sendable {
    private let pushTokensApi: PushTokensApi
    private let auth: AuthRepository

    private let lock = NSLock()
    private var currentToken: String?
    // Account ids whose registration of `currentToken` the server has
    // acknowledged. Cleared when Firebase rotates the token.
    private var registered: Set<String> = []
    // Account ids deliberately unregistered (sign-out in progress): the
    // reconcile loop must not re-register them in the moments between the
    // unregister call and the account actually being removed. Pruned once the
    // account is gone, so a later re-login registers again.
    private var suppressed: Set<String> = []
    private var reconcileTask: Task<Void, Never>?

    private static let baseInterval: Duration = .seconds(2)
    private static let maxInterval: Duration = .seconds(300)
    // Matches Android's bound on the same call: sign-out flows await the
    // unregister, and must never hang on a black-hole network for the full
    // HTTP client timeout.
    private static let unregisterTimeout: Duration = .seconds(3)
    // One slow/stuck register must not stall the whole reconcile pass and
    // starve the other accounts' registrations. Registration is idempotent,
    // so aborting and retrying next tick is safe.
    private static let registerTimeout: Duration = .seconds(10)

    init(pushTokensApi: PushTokensApi, auth: AuthRepository) {
        self.pushTokensApi = pushTokensApi
        self.auth = auth
    }

    func start() {
        reconcileTask = Task { [weak self] in
            // Failed registrations back off exponentially instead of hammering
            // an unreachable (or credential-rejecting) server every tick; the
            // cadence resets as soon as a pass completes without failures.
            var interval = Self.baseInterval
            while !Task.isCancelled {
                guard let self else { return }
                let failed = await self.reconcile()
                interval = failed ? min(interval * 2, Self.maxInterval) : Self.baseInterval
                try? await Task.sleep(for: interval)
            }
        }
    }

    func stop() {
        reconcileTask?.cancel()
        reconcileTask = nil
    }

    /// Messaging delegate callback: stash the (possibly rotated) token. The
    /// reconcile loop posts it for every signed-in account, including ones
    /// that sign in later.
    func register(fcmToken: String) {
        lock.withLock {
            guard currentToken != fcmToken else { return }
            currentToken = fcmToken
            registered.removeAll()
        }
    }

    /// Removes this device's token registration for one account. Must be
    /// awaited BEFORE the account's credentials are dropped — the request
    /// needs the bearer token, and the server scopes the delete to
    /// (token, user), leaving other signed-in accounts' rows intact.
    func unregister(accountId: String) async {
        let token = lock.withLock { () -> String? in
            registered.remove(accountId)
            suppressed.insert(accountId)
            return currentToken
        }
        guard let token else { return }
        do {
            try await Self.withTimeout(Self.unregisterTimeout) {
                try await self.pushTokensApi.unregister(accountId: accountId, token: token)
            }
            logger.info("FCM token unregistered")
        } catch {
            logger.error("Failed to unregister FCM token: \(error.localizedDescription)")
        }
    }

    /// One reconcile pass. Returns true when any register attempt failed, so
    /// the loop can back off.
    private func reconcile() async -> Bool {
        guard let token = lock.withLock({ currentToken }) else { return false }
        let signedIn = Set(auth.accounts.filter { $0.token != nil }.map(\.id))
        let pending = lock.withLock { () -> [String] in
            // Accounts that disappeared may re-register when they return;
            // sign-out removed their server row while credentials existed.
            registered.formIntersection(signedIn)
            suppressed.formIntersection(signedIn)
            return signedIn.subtracting(registered).subtracting(suppressed).sorted()
        }
        var anyFailed = false
        for accountId in pending {
            // Re-check right before the request: a sign-out's unregister or a
            // Firebase token rotation may have happened since the snapshot,
            // and a register issued now would post a row nothing cleans up
            // (or a token that is already dead).
            let proceed = lock.withLock {
                !suppressed.contains(accountId) && currentToken == token
            }
            guard proceed else { continue }
            do {
                try await Self.withTimeout(Self.registerTimeout) {
                    try await self.pushTokensApi.register(accountId: accountId, token: token)
                }
                let signedOutMidFlight = lock.withLock { () -> Bool in
                    if suppressed.contains(accountId) { return true }
                    // Only mark done if the token did not rotate mid-flight:
                    // this call posted the OLD token, and marking the account
                    // registered would skip posting the new one until relaunch
                    // — FCM invalidates the old token and pushes silently die.
                    if currentToken == token { registered.insert(accountId) }
                    return false
                }
                if signedOutMidFlight {
                    // The account signed out while this register was in
                    // flight; its own unregister may have lost the race to
                    // the row this call just (re)created, and once the
                    // credentials are gone nothing else can delete it.
                    try? await Self.withTimeout(Self.registerTimeout) {
                        try await self.pushTokensApi.unregister(accountId: accountId, token: token)
                    }
                } else {
                    logger.info("FCM token registered")
                }
            } catch {
                anyFailed = true
                logger.error("Failed to register FCM token: \(error.localizedDescription)")
            }
        }
        return anyFailed
    }

    /// Races an operation against a deadline; the loser is cancelled. URLSession
    /// requests honor task cancellation, so the abandoned call stops promptly.
    private static func withTimeout<T: Sendable>(
        _ timeout: Duration,
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await withThrowingTaskGroup(of: T.self) { group in
            group.addTask { try await operation() }
            group.addTask {
                try await Task.sleep(for: timeout)
                throw CancellationError()
            }
            let result = try await group.next()!
            group.cancelAll()
            return result
        }
    }
}
