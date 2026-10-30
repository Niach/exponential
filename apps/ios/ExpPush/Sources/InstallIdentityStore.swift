import Foundation
import Security

// EXP-1457 / VAPP-37: the install's eNet identity: 32 ed25519 secret bytes
// whose public half IS this install's iroh endpoint id. Generated once, kept
// in the keychain group the app and its extensions share
// (`AfterFirstUnlockThisDeviceOnly`: never synced, never in a backup, never
// uploaded). The push key is DERIVED from it (`PushKeyStore`), so the X25519
// key the push relay seals to and the one the install registry publishes are
// the same key.

/// Where the identity bytes live: the keychain in the app, memory in tests.
public protocol InstallSecretStorage: Sendable {
    func read(account: String) -> Data?
    /// False when the store refused the write.
    func add(account: String, data: Data) -> Bool
    func delete(account: String)
}

/// One generic-password item per account under `service`, in `group` (nil =
/// the process's default access group).
public struct KeychainInstallSecretStorage: InstallSecretStorage {
    public let service: String
    public let group: String?

    public init(service: String, group: String?) {
        self.service = service
        self.group = group
    }

    private func base(_ account: String) -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        if let group { query[kSecAttrAccessGroup as String] = group }
        return query
    }

    public func read(account: String) -> Data? {
        var query = base(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess else { return nil }
        return result as? Data
    }

    public func add(account: String, data: Data) -> Bool {
        var item = base(account)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        item[kSecValueData as String] = data
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    public func delete(account: String) {
        SecItemDelete(base(account) as CFDictionary)
    }
}

public final class InstallIdentityStore: Sendable {
    static let account = "identity.v1"
    private let storage: InstallSecretStorage

    public init(storage: InstallSecretStorage) {
        self.storage = storage
    }

    /// The shared keychain group (the one `PushKeyStore` uses), staging
    /// detected from the host bundle id.
    public convenience init(bundleIdentifier: String? = Bundle.main.bundleIdentifier) {
        self.init(storage: KeychainInstallSecretStorage(
            service: "at.exponential.enet",
            group: PushKeyStore.sharedGroup(bundleIdentifier: bundleIdentifier)
        ))
    }

    /// The stored identity, or nil (absent, unreadable or malformed).
    public func load() -> Data? {
        guard let data = storage.read(account: Self.account), data.count == 32 else { return nil }
        return data
    }

    /// The stored identity, generating and storing one when there is none.
    /// nil when the keychain refuses the write (an unsigned simulator build
    /// has no access group): no identity, no node, no sealed pushes.
    public func loadOrCreate() -> Data? {
        if let existing = load() { return existing }
        var bytes = Data(count: 32)
        let status = bytes.withUnsafeMutableBytes { SecRandomCopyBytes(kSecRandomDefault, 32, $0.baseAddress!) }
        guard status == errSecSuccess else { return nil }
        if storage.add(account: Self.account, data: bytes) { return bytes }
        // Lost a race with another writer: theirs wins.
        return load()
    }

    /// Forgets the identity: the next `loadOrCreate` makes a new one. Called
    /// once the registry revoked this endpoint id (a revoked id never
    /// registers again) or the last account signed out.
    public func rotate() {
        storage.delete(account: Self.account)
    }
}

/// An in-memory `InstallSecretStorage` (tests, previews).
public final class MemoryInstallSecretStorage: InstallSecretStorage, @unchecked Sendable {
    private let lock = NSLock()
    private var items: [String: Data] = [:]

    public init() {}

    public func read(account: String) -> Data? { lock.withLock { items[account] } }

    public func add(account: String, data: Data) -> Bool {
        lock.withLock {
            guard items[account] == nil else { return false }
            items[account] = data
            return true
        }
    }

    public func delete(account: String) { lock.withLock { _ = items.removeValue(forKey: account) } }
}
