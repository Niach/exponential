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
    public private(set) var needsOnboarding: Bool

    public var isAuthenticated: Bool { token != nil }
    public var hasInstance: Bool { instanceUrl != nil }

    /// True when ANY account still holds a token — the app's nav-gate rule
    /// (login is shown only once every account is tokenless). Surfaces that work
    /// per account (the Share Extension) must gate on this, never on
    /// `isAuthenticated`: a per-server sign-out re-adds the URL as a TOKENLESS
    /// active account, which says nothing about the other signed-in accounts.
    public var hasAuthenticatedAccount: Bool {
        accounts.contains { $0.token != nil }
    }

    /// Ids of the accounts that still hold a token, for callers that resolve
    /// their target account themselves (HTTPClient picks the bearer by
    /// accountId, so a tokenless account can only produce unauthenticated calls).
    public var authenticatedAccountIds: Set<String> {
        Set(accounts.lazy.filter { $0.token != nil }.map(\.id))
    }

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
        self.needsOnboarding = active?.needsOnboarding ?? false
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

    public func setToken(
        _ token: String,
        email: String?,
        name: String? = nil,
        userId: String,
        isAdmin: Bool = false,
        onboardingCompletedAt: String? = nil,
        onboardingKnown: Bool = false
    ) {
        // Per-user account resolution owns onboarding-flag preservation against
        // the *target* account (which may differ from the currently-active one
        // when switching users), so no prior-merge is needed here. The caller
        // guarantees a resolved userId — a login with none never reaches here.
        accountStore.resolveActiveAccount(
            token: token,
            email: email,
            name: name,
            userId: userId,
            isAdmin: isAdmin,
            onboardingCompletedAt: onboardingCompletedAt,
            onboardingKnown: onboardingKnown
        )
        republish()
    }

    /// Marks the active account onboarded (after onboarding.complete succeeds)
    /// so the nav gate stops showing the wizard.
    public func markOnboardingCompleted(_ completedAtIso: String) {
        guard let id = accountStore.activeAccountId else { return }
        accountStore.setOnboardingCompleted(id: id, completedAtIso: completedAtIso)
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

    /// Sign ONE account out locally, keeping its record and its local cache.
    /// This is the dead-session teardown (`SessionGate`): the server-side
    /// session is already gone, so there is nothing to revoke — calling
    /// `authApi.signOut` or the push unregister here would just 401 and re-trip
    /// the gate. Dropping the token is enough: SyncManager cancels the
    /// account's pipeline and the nav gate falls through to LoginView.
    public func signOutLocally(accountId: String) {
        accountStore.clearToken(id: accountId)
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
        needsOnboarding = active?.needsOnboarding ?? false
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
