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

    init(pushTokensApi: PushTokensApi, auth: AuthRepository) {
        self.pushTokensApi = pushTokensApi
        self.auth = auth
    }

    func start() {
        reconcileTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                await self.reconcile()
                try? await Task.sleep(for: .seconds(2))
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
            try await pushTokensApi.unregister(accountId: accountId, token: token)
            logger.info("FCM token unregistered")
        } catch {
            logger.error("Failed to unregister FCM token: \(error.localizedDescription)")
        }
    }

    private func reconcile() async {
        guard let token = lock.withLock({ currentToken }) else { return }
        let signedIn = Set(auth.accounts.filter { $0.token != nil }.map(\.id))
        let pending = lock.withLock { () -> [String] in
            // Accounts that disappeared may re-register when they return;
            // sign-out removed their server row while credentials existed.
            registered.formIntersection(signedIn)
            suppressed.formIntersection(signedIn)
            return signedIn.subtracting(registered).subtracting(suppressed).sorted()
        }
        for accountId in pending {
            do {
                try await pushTokensApi.register(accountId: accountId, token: token)
                lock.withLock { _ = registered.insert(accountId) }
                logger.info("FCM token registered")
            } catch {
                logger.error("Failed to register FCM token: \(error.localizedDescription)")
            }
        }
    }
}
