import Foundation
import Security

// EXP-1459: the session-tree secrets this install holds (sealed session
// content, packages/domain-contract/fixtures/sealed-content.json). One
// keychain item per (root, epoch) in the group the app and its extensions
// share, `AfterFirstUnlockThisDeviceOnly` like the install identity: the
// Notification Service Extension can open sealed pushes on a locked phone
// later, and nothing leaves the device (no sync, no backup). The secrets
// arrive wrapped to this install (`sessionKeys.mine`); ExpCore unwraps them
// and keeps them here.

/// Where tree secrets live: the keychain in the app, memory in tests.
public protocol TreeKeySecretStorage: Sendable {
    /// Every stored item, account → 32 secret bytes.
    func all() -> [String: Data]
    /// Replaces the item. False when the store refused the write.
    func put(account: String, data: Data) -> Bool
    func deleteAll()
    /// One item; nil = none stored; throws when the store cannot be read
    /// (never taken for none).
    func read(account: String) throws -> Data?
}

public struct TreeKeyStorageUnreadable: Error {
    public let status: OSStatus
}

public struct KeychainTreeKeySecretStorage: TreeKeySecretStorage {
    public let service: String
    public let group: String?

    public init(service: String, group: String?) {
        self.service = service
        self.group = group
    }

    private func base() -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
        ]
        if let group { query[kSecAttrAccessGroup as String] = group }
        return query
    }

    public func all() -> [String: Data] {
        var query = base()
        query[kSecReturnAttributes as String] = true
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitAll
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let items = result as? [[String: Any]] else { return [:] }
        var out: [String: Data] = [:]
        for item in items {
            if let account = item[kSecAttrAccount as String] as? String,
               let data = item[kSecValueData as String] as? Data {
                out[account] = data
            }
        }
        return out
    }

    public func read(account: String) throws -> Data? {
        var query = base()
        query[kSecAttrAccount as String] = account
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else { throw TreeKeyStorageUnreadable(status: status) }
        return data
    }

    public func put(account: String, data: Data) -> Bool {
        var query = base()
        query[kSecAttrAccount as String] = account
        SecItemDelete(query as CFDictionary)
        query[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        query[kSecValueData as String] = data
        return SecItemAdd(query as CFDictionary, nil) == errSecSuccess
    }

    public func deleteAll() {
        SecItemDelete(base() as CFDictionary)
    }
}

/// The tree secrets by root id and epoch.
public final class TreeKeyStore: Sendable {
    private let storage: TreeKeySecretStorage

    public init(storage: TreeKeySecretStorage) {
        self.storage = storage
    }

    /// The shared keychain group (the one `PushKeyStore` uses).
    public convenience init(bundleIdentifier: String? = Bundle.main.bundleIdentifier) {
        self.init(storage: KeychainTreeKeySecretStorage(
            service: "at.exponential.treekeys",
            group: PushKeyStore.sharedGroup(bundleIdentifier: bundleIdentifier)
        ))
    }

    static func account(rootId: String, epoch: UInt32) -> String { "\(rootId.lowercased()):\(epoch)" }

    /// Every held secret: rootId (lowercase) → epoch → 32 bytes.
    public func load() -> [String: [UInt32: Data]] {
        var out: [String: [UInt32: Data]] = [:]
        for (account, data) in storage.all() where data.count == 32 {
            guard let colon = account.lastIndex(of: ":"),
                  let epoch = UInt32(account[account.index(after: colon)...]) else { continue }
            out[String(account[..<colon]), default: [:]][epoch] = data
        }
        return out
    }

    @discardableResult
    public func save(rootId: String, epoch: UInt32, secret: Data) -> Bool {
        guard secret.count == 32 else { return false }
        return storage.put(account: Self.account(rootId: rootId, epoch: epoch), data: secret)
    }

    /// Forgets every secret: the install identity rotated (the wraps were
    /// for the old key) or the last account signed out.
    public func removeAll() {
        storage.deleteAll()
    }
}

/// An in-memory `TreeKeySecretStorage` (tests, previews).
public final class MemoryTreeKeySecretStorage: TreeKeySecretStorage, @unchecked Sendable {
    private let lock = NSLock()
    private var items: [String: Data] = [:]

    public init() {}

    public func all() -> [String: Data] { lock.withLock { items } }

    public func read(account: String) throws -> Data? { lock.withLock { items[account] } }

    public func put(account: String, data: Data) -> Bool {
        lock.withLock { items[account] = data }
        return true
    }

    public func deleteAll() { lock.withLock { items.removeAll() } }
}

/// #EXP-1950 (install-trust.json `TrustStore`): the pin store, one JSON item
/// per account, beside the tree keys in the keychain, but in the APP's own
/// access group (`<team>.<bundle id>`, `AfterFirstUnlockThisDeviceOnly`,
/// never synced): no extension reads it (the Notification Service Extension
/// opens pushes with the tree keys alone, the share extension trusts nobody).
public final class InstallTrustPinStore: Sendable {
    /// Apple team prefix (as `PushKeyStore`'s).
    private static let teamPrefix = "V6W7BVCSM8"
    private let storage: TreeKeySecretStorage

    public init(storage: TreeKeySecretStorage) {
        self.storage = storage
    }

    public convenience init(bundleIdentifier: String? = Bundle.main.bundleIdentifier) {
        self.init(storage: KeychainTreeKeySecretStorage(
            service: "at.exponential.installtrust",
            group: bundleIdentifier.map { "\(Self.teamPrefix).\($0)" }
        ))
    }

    /// nil = no store for the account; throws when the keychain cannot be
    /// read (a locked phone before its first unlock): never taken for none.
    public func load(account: String) throws -> Data? { try storage.read(account: account) }

    @discardableResult
    public func save(account: String, data: Data) -> Bool { storage.put(account: account, data: data) }

    public func removeAll() { storage.deleteAll() }
}
