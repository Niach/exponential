import ExpUI
import ExpCore
import ExpPush
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
///
/// EXP-1435: when an account's server names a push relay (`pushTokens.relay`)
/// the registration also carries a relay capability + this device's X25519
/// push public key, and that server's pushes then arrive sealed through APNs
/// (the Notification Service Extension opens them). ONE relay install per
/// relay URL (the APNs device token), one capability per (relay, account).
/// A server without a relay (self-hosted without push) cannot push at all,
/// so nothing is registered there. A relay failure (503 while the relay has no APNs credentials)
/// leaves the account unregistered and retries on its own backoff: NEW-14
/// dropped the bare-token fallback, the server only pushes sealed. The FCM
/// token stays the row key.
final class PushTokenManager: @unchecked Sendable {
    private let pushTokensApi: PushTokensApi
    private let auth: AuthRepository
    private let relayRegistrar: PushRelayRegistrar
    private let pushKeys: PushKeyStore
    private let apnsToken: @Sendable () -> Data?

    /// How an account's current registration was made.
    private enum Mode {
        /// Sealed through the relay.
        case relay
        /// Nothing registered: the server names no relay, so it cannot push.
        case noRelay
    }

    private let lock = NSLock()
    private var currentToken: String?
    // Account ids whose registration of `currentToken` the server has
    // acknowledged, and how. Cleared when Firebase rotates the token or the
    // APNs token changes.
    private var registered: [String: Mode] = [:]
    private var lastApnsToken: Data?
    private var relayRetryAt: ContinuousClock.Instant?
    private var relayBackoff: Duration = PushTokenManager.relayBaseBackoff
    // Account ids whose relay registration failed: held back until
    // `relayRetryAt`, never registered without the relay meanwhile.
    private var awaitingRelay: Set<String> = []
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
    // The relay upgrade retry after a relay failure: POST /installs is
    // throttled per IP (20/hour), so never on the 2 s reconcile cadence.
    private static let relayBaseBackoff: Duration = .seconds(60)
    private static let relayMaxBackoff: Duration = .seconds(6 * 3600)

    init(
        pushTokensApi: PushTokensApi,
        auth: AuthRepository,
        keychain: KeychainStoring,
        apnsToken: @escaping @Sendable () -> Data?
    ) {
        self.pushTokensApi = pushTokensApi
        self.auth = auth
        self.apnsToken = apnsToken
        self.pushKeys = PushKeyStore()
        #if DEBUG
        let environment = "sandbox"
        #else
        let environment = "production"
        #endif
        self.relayRegistrar = PushRelayRegistrar(
            keychain: keychain,
            transport: PushRelayClient(),
            topic: Bundle.main.bundleIdentifier ?? "at.exponential",
            environment: environment
        )
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

    /// The push key changed (EXP-1457: the install identity rotated): every
    /// account registers the new public key on the next tick.
    func resetRegistrations() {
        lock.withLock { registered.removeAll() }
    }

    /// Removes this device's token registration for one account. Must be
    /// awaited BEFORE the account's credentials are dropped — the request
    /// needs the bearer token, and the server scopes the delete to
    /// (token, user), leaving other signed-in accounts' rows intact.
    func unregister(accountId: String) async {
        let token = lock.withLock { () -> String? in
            registered.removeValue(forKey: accountId)
            suppressed.insert(accountId)
            return currentToken
        }
        // The row delete and the relay capability revoke (best effort; it
        // needs no account credentials) run side by side, each bounded.
        let registrar = relayRegistrar
        async let revoke: Void = Self.revokeCapability(registrar, accountId: accountId)
        if let token {
            do {
                try await Self.withTimeout(Self.unregisterTimeout) {
                    try await self.pushTokensApi.unregister(accountId: accountId, token: token)
                }
                logger.info("FCM token unregistered")
            } catch {
                logger.error("Failed to unregister FCM token: \(error.localizedDescription)")
            }
        }
        await revoke
    }

    private static func revokeCapability(_ registrar: PushRelayRegistrar, accountId: String) async {
        _ = try? await withTimeout(unregisterTimeout) {
            await registrar.revoke(accountId: accountId)
        }
    }

    /// One reconcile pass. Returns true when any register attempt failed, so
    /// the loop can back off.
    private func reconcile() async -> Bool {
        guard let token = lock.withLock({ currentToken }) else { return false }
        let signedIn = Set(auth.accounts.filter { $0.token != nil }.map(\.id))
        let apns = apnsToken()
        let now = ContinuousClock.now
        let pending = lock.withLock { () -> [String] in
            // A new APNs device token re-registers everyone: the relay
            // install's token must follow it (PUT).
            if apns != lastApnsToken {
                lastApnsToken = apns
                registered.removeAll()
            }
            // Accounts that disappeared may re-register when they return;
            // sign-out removed their server row while credentials existed.
            registered = registered.filter { signedIn.contains($0.key) }
            suppressed.formIntersection(signedIn)
            awaitingRelay.formIntersection(signedIn)
            // An account whose relay failed retries once the relay backoff
            // has passed.
            let relayHeld = relayRetryAt.map { now < $0 } ?? false
            return signedIn.subtracting(registered.keys).subtracting(suppressed)
                .subtracting(relayHeld ? awaitingRelay : []).sorted()
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
                guard let mode = try await registerOne(accountId: accountId, token: token, apns: apns) else {
                    continue
                }
                let signedOutMidFlight = lock.withLock { () -> Bool in
                    awaitingRelay.remove(accountId)
                    if suppressed.contains(accountId) { return true }
                    // Only mark done if the token did not rotate mid-flight:
                    // this call posted the OLD token, and marking the account
                    // registered would skip posting the new one until relaunch
                    // — FCM invalidates the old token and pushes silently die.
                    if currentToken == token { registered[accountId] = mode }
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
                    await Self.revokeCapability(relayRegistrar, accountId: accountId)
                } else {
                    logger.info("FCM token registered (\(Self.describe(mode), privacy: .public))")
                }
            } catch {
                anyFailed = true
                logger.error("Failed to register FCM token: \(error.localizedDescription)")
            }
        }
        return anyFailed
    }

    /// Registers one account sealed through its server's relay; a server
    /// that names none gets nothing (it cannot push). Returns nil when a relay step
    /// failed: the account stays unregistered until the relay retry window
    /// passes (NEW-14: no bare-token fallback). Throws when the server
    /// cannot be asked for its relay.
    private func registerOne(accountId: String, token: String, apns: Data?) async throws -> Mode? {
        let relay = try await Self.withTimeout(Self.registerTimeout) {
            try await self.pushTokensApi.relayUrl(accountId: accountId)
        }
        guard let relay else { return .noRelay }
        do {
            guard let keypair = pushKeys.loadOrCreate() else { throw PushRelayError.invalidResponse }
            // Firebase hands over the APNs token before the FCM token; if it
            // is still missing, retry later.
            guard let apns else { throw PushRelayError.invalidResponse }
            let label = auth.accounts.first(where: { $0.id == accountId })
                .flatMap { URL(string: $0.instanceUrl)?.host } ?? "Exponential"
            let registrar = relayRegistrar
            let hex = apns.map { String(format: "%02x", $0) }.joined()
            let result = try await Self.withTimeout(Self.registerTimeout) {
                try await registrar.capability(relay: relay, accountId: accountId, label: label, apnsToken: hex)
            }
            if !result.resetAccounts.isEmpty {
                // Their capability died with the old install: their rows must
                // register the new one.
                lock.withLock {
                    for id in result.resetAccounts { registered.removeValue(forKey: id) }
                }
            }
            try await Self.withTimeout(Self.registerTimeout) {
                try await self.pushTokensApi.register(
                    accountId: accountId,
                    token: token,
                    capability: result.capability,
                    pushPublicKey: keypair.publicKeyBase64Url
                )
            }
            lock.withLock {
                relayRetryAt = nil
                relayBackoff = Self.relayBaseBackoff
            }
            return .relay
        } catch {
            logger.error("Push relay registration failed, retrying later: \(error.localizedDescription)")
            lock.withLock {
                awaitingRelay.insert(accountId)
                // One step of backoff per retry window, however many accounts
                // hit the failure in it.
                let now = ContinuousClock.now
                if relayRetryAt.map({ now >= $0 }) ?? true {
                    relayRetryAt = now + relayBackoff
                    relayBackoff = min(relayBackoff * 2, Self.relayMaxBackoff)
                }
            }
            return nil
        }
    }

    private static func describe(_ mode: Mode) -> String {
        switch mode {
        case .relay: "sealed via relay"
        case .noRelay: "skipped, the server has no relay"
        }
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
