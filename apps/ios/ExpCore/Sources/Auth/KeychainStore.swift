import Foundation
import Security

/// Keychain-backed string store, shared between the app and the Share Extension
/// via a keychain access group ([SharedAppGroup.keychainAccessGroup]).
///
/// Existing installs wrote items WITHOUT an access group (they landed in the
/// app's default group, which the extension can't read). To migrate without
/// logging anyone out, reads fall back to the legacy (no-group) item and
/// promote it into the shared group; writes always target the shared group and
/// clear the legacy copy. `AccountStore.init` calls `persist()` on first launch,
/// so accounts move into the shared group immediately after the update.
public final class KeychainStore: Sendable {
    private let service = "at.exponential"
    private let accessGroup: String? = SharedAppGroup.keychainAccessGroup

    public init() {}

    public func get(_ key: String) -> String? {
        // Prefer the shared-group item.
        if let value = read(key, useAccessGroup: true) {
            return value
        }
        // Legacy fallback: an item written before the shared group existed.
        if let legacy = read(key, useAccessGroup: false) {
            // Promote it into the shared group so the extension can see it next time.
            set(key, value: legacy)
            return legacy
        }
        return nil
    }

    public func set(_ key: String, value: String?) {
        // Clear both groups, then write the shared-group copy only.
        deleteItem(key, useAccessGroup: true)
        deleteItem(key, useAccessGroup: false)
        guard let value, let data = value.data(using: .utf8) else { return }
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock,
        ]
        if let accessGroup { query[kSecAttrAccessGroup as String] = accessGroup }
        SecItemAdd(query as CFDictionary, nil)
    }

    public func delete(_ key: String) {
        deleteItem(key, useAccessGroup: true)
        deleteItem(key, useAccessGroup: false)
    }

    // MARK: - Private

    private func read(_ key: String, useAccessGroup: Bool) -> String? {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        if useAccessGroup, let accessGroup { query[kSecAttrAccessGroup as String] = accessGroup }
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess, let data = result as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private func deleteItem(_ key: String, useAccessGroup: Bool) {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
        ]
        if useAccessGroup, let accessGroup { query[kSecAttrAccessGroup as String] = accessGroup }
        SecItemDelete(query as CFDictionary)
    }
}
