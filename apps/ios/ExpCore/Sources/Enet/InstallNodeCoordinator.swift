import Foundation
import GRDB

/// Drives `InstallNodeService` from the app's state: the signed-in accounts
/// and the teams in their local databases, checked every few seconds (cheap
/// local reads), so a sign-in, an account switch or a team join/leave syncs
/// the node within one tick; the allowlist reloads from the server on every
/// change and at least every 10 minutes. Runs on its own task, never on the
/// main thread.
public final class InstallNodeCoordinator: @unchecked Sendable {
    public static let refreshInterval: Duration = .seconds(600)
    private static let tick: Duration = .seconds(5)

    private let service: InstallNodeService
    private let auth: AuthRepository
    private let db: DatabaseManager
    private let lock = NSLock()
    private var task: Task<Void, Never>?
    // Accounts signing out: never re-registered while their credentials are
    // still around (the registry just revoked this endpoint for them).
    private var suppressed: Set<String> = []

    public init(service: InstallNodeService, auth: AuthRepository, db: DatabaseManager) {
        self.service = service
        self.auth = auth
        self.db = db
    }

    public func start() {
        lock.withLock {
            guard task == nil else { return }
            task = Task.detached(priority: .utility) { [weak self] in
                var last: [InstallAccount]?
                var refreshedAt = ContinuousClock.now
                while !Task.isCancelled {
                    guard let self else { return }
                    let current = self.snapshot()
                    let now = ContinuousClock.now
                    if current != last {
                        last = current
                        refreshedAt = now
                        await self.service.sync(current)
                    } else if !current.isEmpty, now - refreshedAt >= Self.refreshInterval {
                        refreshedAt = now
                        await self.service.sync(current)
                    }
                    try? await Task.sleep(for: Self.tick)
                }
            }
        }
    }

    public func stop() {
        lock.withLock {
            task?.cancel()
            task = nil
        }
        Task { await service.stop() }
    }

    /// An explicit sign-out of this install from one account: revokes the
    /// install there. Await it BEFORE the credentials are dropped (the call
    /// needs the bearer token).
    public func signOut(accountId: String) async {
        lock.withLock { _ = suppressed.insert(accountId) }
        await service.signOut(accountId: accountId, revoke: true)
    }

    private func snapshot() -> [InstallAccount] {
        let signedIn = auth.accounts.filter { $0.token != nil }
        let ids = Set(signedIn.map(\.id))
        let skip = lock.withLock { () -> Set<String> in
            suppressed.formIntersection(ids)
            return suppressed
        }
        return signedIn
            .filter { !skip.contains($0.id) }
            .compactMap { account -> InstallAccount? in
                guard let userId = account.userId else { return nil }
                let teams = (try? db.poolIfOpen(forAccountId: account.id)?.read { db in
                    try String.fetchAll(db, sql: "SELECT id FROM teams ORDER BY id")
                }) ?? []
                return InstallAccount(accountId: account.id, userId: userId, teamIds: teams)
            }
            .sorted { $0.accountId < $1.accountId }
    }
}
