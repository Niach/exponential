import Foundation

private let keyAccounts = "accounts"
private let keyActiveAccountId = "active_account_id"
// One-shot flag: URL-keyed accounts have been re-keyed to per-user ids.
private let keyPerUserMigrationDone = "peruser_migration_v1"

// Legacy single-account keys, migrated on first launch.
private let legacyKeyInstanceUrl = "instance_url"
private let legacyKeyToken = "session_token"
private let legacyKeyUserEmail = "user_email"
private let legacyKeyUserName = "user_name"
private let legacyKeyUserId = "user_id"
private let legacyKeyIsAdmin = "is_admin"

public final class AccountStore: @unchecked Sendable {
    private let store: any KeychainStoring
    private let lock = NSLock()
    private var cached: [ServerAccount]
    private var cachedActiveId: String?

    public init(keychain: any KeychainStoring) {
        self.store = keychain
        self.cached = AccountStore.loadAccounts(from: keychain)
        self.cachedActiveId = keychain.get(keyActiveAccountId)
        AccountStore.migrateLegacyIfNeeded(store: keychain, accounts: &self.cached, activeId: &self.cachedActiveId)
        AccountStore.migratePerUserIdsIfNeeded(store: keychain, accounts: &self.cached, activeId: &self.cachedActiveId)
        persist()
    }

    public var accounts: [ServerAccount] {
        lock.withLock { cached }
    }

    public var activeAccount: ServerAccount? {
        lock.withLock {
            guard let id = cachedActiveId else { return nil }
            return cached.first { $0.id == id }
        }
    }

    public var activeAccountId: String? {
        lock.withLock { cachedActiveId }
    }

    /// Adds (or merges) an account by instance URL and marks it active.
    /// Returns the resulting account, with a stable id.
    @discardableResult
    public func upsertAndActivate(instanceUrl: String) -> ServerAccount {
        lock.lock()
        defer { lock.unlock() }
        let id = ServerAccount.makeId(for: instanceUrl)
        if let idx = cached.firstIndex(where: { $0.id == id }) {
            cached[idx].instanceUrl = instanceUrl
            cached[idx].lastUsedAt = Date()
        } else {
            cached.append(ServerAccount(
                id: id,
                instanceUrl: instanceUrl,
                token: nil,
                userEmail: nil,
                userName: nil,
                userId: nil,
                isAdmin: false,
                lastUsedAt: Date()
            ))
        }
        cachedActiveId = id
        persistLocked()
        return cached.first { $0.id == id }!
    }

    /// Resolve which account a successful login belongs to (per-user identity)
    /// and persist its token + fields. Implements the account state machine:
    /// - same user on the active server → in-place token refresh (no DB wipe);
    /// - a per-user account that already exists → switch to it (offline cache
    ///   preserved), dropping the tokenless pending record we signed in from;
    /// - a pending (pre-login) record → re-keyed to the per-user id, so the
    ///   fresh DB file triggers a full snapshot sync;
    /// - another user already resolved on this server → a fresh per-user record
    ///   rather than clobbering theirs.
    /// The caller MUST pass a resolved userId (the login VMs fail the login if a
    /// session never resolves one). Returns the resolved account id.
    @discardableResult
    public func resolveActiveAccount(
        token: String,
        email: String?,
        name: String?,
        userId: String,
        isAdmin: Bool,
        onboardingCompletedAt: String?,
        onboardingKnown: Bool
    ) -> String {
        lock.lock()
        defer { lock.unlock() }

        guard let activeId = cachedActiveId,
              let activeIdx = cached.firstIndex(where: { $0.id == activeId }) else {
            return cachedActiveId ?? ""
        }
        let instanceUrl = cached[activeIdx].instanceUrl
        let perUserId = ServerAccount.makeId(instanceUrl: instanceUrl, userId: userId)

        if let existingIdx = cached.firstIndex(where: { $0.id == perUserId }) {
            applyResolvedFields(
                at: existingIdx, token: token, email: email, name: name,
                userId: userId, isAdmin: isAdmin,
                onboardingCompletedAt: onboardingCompletedAt, onboardingKnown: onboardingKnown
            )
            cachedActiveId = perUserId
            // Drop the distinct tokenless/userless pending record we came from.
            if activeId != perUserId, cached[activeIdx].token == nil, cached[activeIdx].userId == nil {
                cached.removeAll { $0.id == activeId }
            }
            persistLocked()
            return perUserId
        }

        if cached[activeIdx].userId == nil {
            // Pending (pre-login) record → re-key to the per-user id.
            cached[activeIdx].id = perUserId
            applyResolvedFields(
                at: activeIdx, token: token, email: email, name: name,
                userId: userId, isAdmin: isAdmin,
                onboardingCompletedAt: onboardingCompletedAt, onboardingKnown: onboardingKnown
            )
            cachedActiveId = perUserId
        } else {
            // Another user is already resolved on this server — create a fresh
            // per-user record instead of clobbering theirs.
            cached.append(ServerAccount(
                id: perUserId,
                instanceUrl: instanceUrl,
                token: token,
                userEmail: email,
                userName: name,
                userId: userId,
                isAdmin: isAdmin,
                onboardingCompletedAt: onboardingCompletedAt,
                onboardingKnown: onboardingKnown ? true : nil,
                lastUsedAt: Date()
            ))
            cachedActiveId = perUserId
        }
        persistLocked()
        return perUserId
    }

    /// Assign the resolved login fields onto `cached[idx]`, preserving a prior
    /// onboarding flag when the incoming session didn't report one (a transient
    /// session read must never bounce a returning user back to the wizard).
    /// Assumes the store lock is already held.
    private func applyResolvedFields(
        at idx: Int, token: String, email: String?, name: String?, userId: String,
        isAdmin: Bool, onboardingCompletedAt: String?, onboardingKnown: Bool
    ) {
        let priorCompleted = cached[idx].onboardingCompletedAt
        let priorKnown = cached[idx].onboardingKnown
        cached[idx].token = token
        cached[idx].userEmail = email
        cached[idx].userName = name
        cached[idx].userId = userId
        cached[idx].isAdmin = isAdmin
        cached[idx].onboardingCompletedAt = onboardingCompletedAt ?? priorCompleted
        cached[idx].onboardingKnown = (onboardingKnown || priorKnown == true) ? true : nil
        cached[idx].lastUsedAt = Date()
    }

    /// Marks an account onboarded (after onboarding.complete succeeds) so the
    /// nav gate stops showing the wizard without waiting for a session re-read.
    public func setOnboardingCompleted(id: String, completedAtIso: String) {
        lock.lock()
        defer { lock.unlock() }
        guard let idx = cached.firstIndex(where: { $0.id == id }) else { return }
        cached[idx].onboardingCompletedAt = completedAtIso
        cached[idx].onboardingKnown = true
        persistLocked()
    }

    /// Switches the active account to the given id. No-op if id is unknown.
    public func setActive(id: String) {
        lock.lock()
        defer { lock.unlock() }
        guard cached.contains(where: { $0.id == id }) else { return }
        cachedActiveId = id
        if let idx = cached.firstIndex(where: { $0.id == id }) {
            cached[idx].lastUsedAt = Date()
        }
        persistLocked()
    }

    /// Removes an account. If the removed account was active, picks the most-recent remaining
    /// account as the new active, or nil if none remain.
    public func remove(id: String) {
        lock.lock()
        defer { lock.unlock() }
        cached.removeAll { $0.id == id }
        if cachedActiveId == id {
            cachedActiveId = cached.max(by: { $0.lastUsedAt < $1.lastUsedAt })?.id
        }
        persistLocked()
    }

    // MARK: - Persistence

    private func persist() {
        lock.lock()
        defer { lock.unlock() }
        persistLocked()
    }

    private func persistLocked() {
        if let data = try? JSONEncoder().encode(cached), let json = String(data: data, encoding: .utf8) {
            store.set(keyAccounts, value: json)
        }
        store.set(keyActiveAccountId, value: cachedActiveId)
    }

    private static func loadAccounts(from keychain: any KeychainStoring) -> [ServerAccount] {
        guard
            let json = keychain.get(keyAccounts),
            let data = json.data(using: .utf8),
            let accounts = try? JSONDecoder().decode([ServerAccount].self, from: data)
        else { return [] }
        return accounts
    }

    private static func migrateLegacyIfNeeded(
        store: any KeychainStoring,
        accounts: inout [ServerAccount],
        activeId: inout String?
    ) {
        // If we already have any accounts, skip migration.
        guard accounts.isEmpty else { return }
        guard let legacyUrl = store.get(legacyKeyInstanceUrl) else { return }
        let id = ServerAccount.makeId(for: legacyUrl)
        let account = ServerAccount(
            id: id,
            instanceUrl: legacyUrl,
            token: store.get(legacyKeyToken),
            userEmail: store.get(legacyKeyUserEmail),
            userName: store.get(legacyKeyUserName),
            userId: store.get(legacyKeyUserId),
            isAdmin: store.get(legacyKeyIsAdmin) == "true",
            lastUsedAt: Date()
        )
        accounts = [account]
        activeId = id
        store.delete(legacyKeyInstanceUrl)
        store.delete(legacyKeyToken)
        store.delete(legacyKeyUserEmail)
        store.delete(legacyKeyUserName)
        store.delete(legacyKeyUserId)
        store.delete(legacyKeyIsAdmin)
    }

    // One-shot re-key of URL-keyed accounts to per-user ids. Before this, two
    // users on the same server shared one accountId (hence one DB file → "logged
    // into the wrong account"). Re-key each signed-in URL-keyed record to
    // makeId(instanceUrl:userId:); a signed-in record with no captured userId
    // gets its token nulled to force a clean re-login. The stale URL-keyed DB
    // files are wiped app-side (AppDependencies) — the cache may be the wrong
    // user's data, so it's deleted, not renamed. Guarded by a persisted flag;
    // idempotent (the keychain survives reinstall).
    static func migratePerUserIdsIfNeeded(
        store: any KeychainStoring,
        accounts: inout [ServerAccount],
        activeId: inout String?
    ) {
        guard store.get(keyPerUserMigrationDone) != "true" else { return }
        for idx in accounts.indices {
            let account = accounts[idx]
            // Only touch legacy URL-keyed records; per-user ids already differ.
            guard account.id == ServerAccount.makeId(for: account.instanceUrl) else { continue }
            guard account.token != nil else { continue }
            if let userId = account.userId, !userId.isEmpty {
                let perUserId = ServerAccount.makeId(instanceUrl: account.instanceUrl, userId: userId)
                if perUserId != account.id {
                    if activeId == account.id { activeId = perUserId }
                    accounts[idx].id = perUserId
                }
            } else {
                // Signed-in but no userId — can't derive a per-user id. Null the
                // token so the next launch re-authenticates cleanly.
                accounts[idx].token = nil
            }
        }
        store.set(keyPerUserMigrationDone, value: "true")
    }
}
