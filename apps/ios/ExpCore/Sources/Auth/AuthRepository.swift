import Foundation

@Observable
public final class AuthRepository: @unchecked Sendable {
    private let accountStore: AccountStore

    // Mirrored from accountStore.activeAccount so @Observable can publish changes.
    public private(set) var accounts: [ServerAccount]
    public private(set) var activeAccountId: String?
    public private(set) var instanceUrl: String?
    public private(set) var token: String?
    public private(set) var userEmail: String?
    public private(set) var userName: String?
    public private(set) var userId: String?
    public private(set) var isAdmin: Bool

    public var isAuthenticated: Bool { token != nil }
    public var hasInstance: Bool { instanceUrl != nil }

    public init(accountStore: AccountStore) {
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

    public func setInstanceUrl(_ url: String) {
        let normalized = normalizeBaseUrl(url)
        accountStore.upsertAndActivate(instanceUrl: normalized)
        republish()
    }

    public func clearInstanceUrl() {
        // Used by the legacy "change instance" flow — equivalent to removing the active account.
        if let id = accountStore.activeAccountId {
            accountStore.remove(id: id)
        }
        republish()
    }

    /// Resolves the API base URL for an account, falling back to the active
    /// instance URL. Shared by the issue detail and create-issue screens.
    public func instanceBaseURL(forAccountId accountId: String) -> URL? {
        let urlString =
            accounts.first(where: { $0.id == accountId })?.instanceUrl ?? instanceUrl
        return urlString.flatMap { URL(string: $0) }
    }

    // MARK: - Token

    public func setToken(_ token: String, email: String?, name: String? = nil, userId: String? = nil, isAdmin: Bool = false) {
        accountStore.updateActiveToken(token: token, email: email, name: name, userId: userId, isAdmin: isAdmin)
        republish()
    }

    // MARK: - Multi-account

    public func switchAccount(id: String) {
        accountStore.setActive(id: id)
        republish()
    }

    public func removeAccount(id: String) {
        accountStore.remove(id: id)
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
