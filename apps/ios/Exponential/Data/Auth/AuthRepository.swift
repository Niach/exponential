import Foundation

private let keyInstanceUrl = "instance_url"
private let keyToken = "session_token"
private let keyUserEmail = "user_email"
private let keyUserName = "user_name"
private let keyUserId = "user_id"
private let keyIsAdmin = "is_admin"

@Observable
final class AuthRepository: @unchecked Sendable {
    private let store: KeychainStore

    private(set) var instanceUrl: String?
    private(set) var token: String?
    private(set) var userEmail: String?
    private(set) var userName: String?
    private(set) var userId: String?
    private(set) var isAdmin: Bool

    var isAuthenticated: Bool { token != nil }
    var hasInstance: Bool { instanceUrl != nil }

    init(keychain: KeychainStore) {
        self.store = keychain
        self.instanceUrl = keychain.get(keyInstanceUrl)
        self.token = keychain.get(keyToken)
        self.userEmail = keychain.get(keyUserEmail)
        self.userName = keychain.get(keyUserName)
        self.userId = keychain.get(keyUserId)
        self.isAdmin = keychain.get(keyIsAdmin) == "true"
    }

    func setInstanceUrl(_ url: String) {
        let normalized = normalizeBaseUrl(url)
        store.set(keyInstanceUrl, value: normalized)
        instanceUrl = normalized
    }

    func clearInstanceUrl() {
        store.delete(keyInstanceUrl)
        instanceUrl = nil
        clearToken()
    }

    func setToken(_ token: String, email: String?, name: String? = nil, userId: String? = nil, isAdmin: Bool = false) {
        store.set(keyToken, value: token)
        store.set(keyUserEmail, value: email)
        store.set(keyUserName, value: name)
        store.set(keyUserId, value: userId)
        store.set(keyIsAdmin, value: isAdmin ? "true" : nil)
        self.token = token
        self.userEmail = email
        self.userName = name
        self.userId = userId
        self.isAdmin = isAdmin
    }

    func clearToken() {
        store.delete(keyToken)
        store.delete(keyUserEmail)
        store.delete(keyUserName)
        store.delete(keyUserId)
        store.delete(keyIsAdmin)
        token = nil
        userEmail = nil
        userName = nil
        userId = nil
        isAdmin = false
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
