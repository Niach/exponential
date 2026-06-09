import Foundation

private let keyAccounts = "accounts"
private let keyActiveAccountId = "active_account_id"

// Legacy single-account keys, migrated on first launch.
private let legacyKeyInstanceUrl = "instance_url"
private let legacyKeyToken = "session_token"
private let legacyKeyUserEmail = "user_email"
private let legacyKeyUserName = "user_name"
private let legacyKeyUserId = "user_id"
private let legacyKeyIsAdmin = "is_admin"

public final class AccountStore: @unchecked Sendable {
    private let store: KeychainStore
    private let lock = NSLock()
    private var cached: [ServerAccount]
    private var cachedActiveId: String?

    public init(keychain: KeychainStore) {
        self.store = keychain
        self.cached = AccountStore.loadAccounts(from: keychain)
        self.cachedActiveId = keychain.get(keyActiveAccountId)
        AccountStore.migrateLegacyIfNeeded(store: keychain, accounts: &self.cached, activeId: &self.cachedActiveId)
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

    /// Updates the token + user info on the active account.
    public func updateActiveToken(
        token: String,
        email: String?,
        name: String?,
        userId: String?,
        isAdmin: Bool,
        onboardingCompletedAt: String? = nil,
        onboardingKnown: Bool? = nil
    ) {
        lock.lock()
        defer { lock.unlock() }
        guard let id = cachedActiveId, let idx = cached.firstIndex(where: { $0.id == id }) else { return }
        cached[idx].token = token
        cached[idx].userEmail = email
        cached[idx].userName = name
        cached[idx].userId = userId
        cached[idx].isAdmin = isAdmin
        cached[idx].onboardingCompletedAt = onboardingCompletedAt
        cached[idx].onboardingKnown = onboardingKnown
        cached[idx].lastUsedAt = Date()
        persistLocked()
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

    private static func loadAccounts(from keychain: KeychainStore) -> [ServerAccount] {
        guard
            let json = keychain.get(keyAccounts),
            let data = json.data(using: .utf8),
            let accounts = try? JSONDecoder().decode([ServerAccount].self, from: data)
        else { return [] }
        return accounts
    }

    private static func migrateLegacyIfNeeded(
        store: KeychainStore,
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
}
