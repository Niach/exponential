import ExpCore
import Foundation
import os

private let logger = Logger(subsystem: "at.exponential", category: "TimezoneClaimer")

/// EXP-452: stamps the device's IANA timezone on every signed-in account,
/// once per launch (`users.setTimezone` with `onlyIfUnset` — an explicit pick
/// in web/desktop settings always wins). The daily digest's send hour is read
/// in `users.timezone`; web and desktop claim it at sign-in, but an account
/// that only ever signed in on mobile stayed NULL and had its digest
/// silently scheduled on UTC's clock (a 10:00 delivery for a "8:00" German
/// account). Mirrors PushTokenManager's polling reconcile over
/// `auth.accounts` so an account added after launch is claimed within one
/// tick instead of waiting for the next cold start.
final class TimezoneClaimer: @unchecked Sendable {
    private let usersApi: UsersApi
    private let auth: AuthRepository

    private let lock = NSLock()
    // Account ids whose claim the server acknowledged this launch. Never
    // cleared: the zone is claim-once by design (`onlyIfUnset`), so a
    // re-claim after an account switch would be a no-op anyway.
    private var claimed: Set<String> = []
    private var task: Task<Void, Never>?

    private static let baseInterval: Duration = .seconds(5)
    private static let maxInterval: Duration = .seconds(300)

    init(usersApi: UsersApi, auth: AuthRepository) {
        self.usersApi = usersApi
        self.auth = auth
    }

    func start() {
        task = Task { [weak self] in
            // Failed claims (offline at launch, an older server without the
            // route) back off exponentially instead of retrying every tick;
            // the cadence resets once a pass completes without failures.
            var interval = Self.baseInterval
            while !Task.isCancelled {
                guard let self else { return }
                let failed = await self.claimPending()
                interval = failed ? min(interval * 2, Self.maxInterval) : Self.baseInterval
                try? await Task.sleep(for: interval)
            }
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }

    /// One pass: claim for every signed-in account not yet acknowledged.
    /// Steady state is an empty pending set — no network traffic at all.
    /// Returns whether any claim failed (drives the backoff).
    private func claimPending() async -> Bool {
        let timezone = TimeZone.current.identifier
        let pending = lock.withLock {
            auth.authenticatedAccountIds.subtracting(claimed)
        }
        var failed = false
        for accountId in pending {
            do {
                try await usersApi.setTimezone(
                    accountId: accountId,
                    timezone: timezone,
                    onlyIfUnset: true
                )
                lock.withLock { _ = claimed.insert(accountId) }
            } catch {
                // Best-effort — a missed claim just means the digest reads
                // UTC until the next successful pass or launch.
                failed = true
                logger.warning("timezone claim failed: \(error.localizedDescription)")
            }
        }
        return failed
    }
}
