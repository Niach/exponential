import Foundation

@Observable
final class AuthRepository: @unchecked Sendable {
    private let accountStore: AccountStore

    // Mirrored from accountStore.activeAccount so @Observable can publish changes.
    private(set) var accounts: [ServerAccount]
    private(set) var activeAccountId: String?
    private(set) var instanceUrl: String?
    private(set) var token: String?
    private(set) var userEmail: String?
    private(set) var userName: String?
    private(set) var userId: String?
    private(set) var isAdmin: Bool

    var isAuthenticated: Bool { token != nil }
    var hasInstance: Bool { instanceUrl != nil }

    init(accountStore: AccountStore) {
        self.accountStore = accountStore
        self.accounts = accountStore.accounts
        self.activeAccountId = accountStore.activeAccountId
        let active = accountStore.activeAccount
        self.instanceUrl = active?.instanceUrl
        self.token = active?.token
        self.userEmail = active?.userEmail
        self.userName = active?.userName
        self.userId = active?.userId
        self.isAdmin = active?.isAdmin ?? false
    }

    // MARK: - Instance URL

    func setInstanceUrl(_ url: String) {
        let normalized = normalizeBaseUrl(url)
        accountStore.upsertAndActivate(instanceUrl: normalized)
        republish()
    }

    func clearInstanceUrl() {
        // Used by the legacy "change instance" flow — equivalent to removing the active account.
        if let id = accountStore.activeAccountId {
            accountStore.remove(id: id)
        }
        republish()
    }

    // MARK: - Token

    func setToken(_ token: String, email: String?, name: String? = nil, userId: String? = nil, isAdmin: Bool = false) {
        accountStore.updateActiveToken(token: token, email: email, name: name, userId: userId, isAdmin: isAdmin)
        republish()
    }

    func clearToken() {
        accountStore.clearActiveToken()
        republish()
    }

    // MARK: - Multi-account

    func switchAccount(id: String) {
        accountStore.setActive(id: id)
        republish()
    }

    func removeAccount(id: String) {
        accountStore.remove(id: id)
        republish()
    }

    /// Drives the "add server" flow. Locally clears the published instance/token state so
    /// AppNavigator routes through InstanceView → LoginView, while leaving the underlying
    /// AccountStore untouched so cancelAddServer() can restore the prior active account.
    private(set) var isAddingServer: Bool = false

    func startAddServer() {
        isAddingServer = true
        activeAccountId = nil
        instanceUrl = nil
        token = nil
        userEmail = nil
        userName = nil
        userId = nil
        isAdmin = false
    }

    func cancelAddServer() {
        isAddingServer = false
        republish()
    }

    // MARK: - Internals

    private func republish() {
        accounts = accountStore.accounts
        activeAccountId = accountStore.activeAccountId
        let active = accountStore.activeAccount
        instanceUrl = active?.instanceUrl
        token = active?.token
        userEmail = active?.userEmail
        userName = active?.userName
        userId = active?.userId
        isAdmin = active?.isAdmin ?? false
        isAddingServer = false
    }

    private func normalizeBaseUrl(_ input: String) -> String {
        var trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        while trimmed.hasSuffix("/") {
            trimmed.removeLast()
        }
        if !trimmed.hasPrefix("http://") && !trimmed.hasPrefix("https://") {
            trimmed = "https://\(trimmed)"
        }
        return trimmed
    }
}
