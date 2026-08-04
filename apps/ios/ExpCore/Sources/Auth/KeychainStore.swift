import Foundation
import Security
import os

private let logger = Logger(subsystem: "at.exponential", category: "KeychainStore")

/// A keychain read failed for a reason OTHER than "no such item" — the
/// keychain is locked, the entitlement is missing, or the item is otherwise
/// unreadable. Callers must treat this as "state unknown", never as "empty":
/// persisting over an unreadable keychain is how sessions get destroyed
/// (EXP-405 — build 77 shipped without the keychain-access-groups entitlement
/// and every group-scoped call failed; the old void API swallowed that).
public struct KeychainReadError: Error {
    public let status: OSStatus
}

/// The minimal key/value surface `AccountStore` needs. Extracted so tests can
/// substitute an in-memory fake for the real (entitlement-bound) keychain.
public protocol KeychainStoring: Sendable {
    /// nil means the key is definitively absent (`errSecItemNotFound`);
    /// any other failure THROWS `KeychainReadError`.
    func get(_ key: String) throws -> String?
    /// Returns false when the write could not be confirmed. A failed write
    /// must leave any previously stored value intact.
    @discardableResult
    func set(_ key: String, value: String?) -> Bool
    func delete(_ key: String)
}

/// Keychain-backed string store, shared between the app and the Share Extension
/// via a keychain access group ([SharedAppGroup.keychainAccessGroup]).
///
/// Existing installs wrote items into the app's default group (which the
/// extension can't read). To migrate without logging anyone out, reads fall
/// back to that legacy group and promote the item into the shared group; the
/// legacy copy is deleted only AFTER the shared-group write is confirmed.
/// Writes are update-or-add — never delete-then-add, which loses the credential
/// whenever the add fails — and every non-success OSStatus is logged so a
/// broken keychain names itself in Console instead of silently signing the
/// user out (EXP-405).
///
/// EVERY query here names its access group explicitly. A query without
/// `kSecAttrAccessGroup` matches items in ALL groups the process can access,
/// so an "ungrouped" legacy delete after a shared-group write erases the item
/// just written — that was the 0.14.0(80) logout-on-relaunch bug.
public final class KeychainStore: KeychainStoring, Sendable {
    private let service = "at.exponential"
    private let sharedGroup = SharedAppGroup.keychainAccessGroup
    private let legacyGroup = SharedAppGroup.keychainLegacyAccessGroup

    public init() {}

    public func get(_ key: String) throws -> String? {
        // Prefer the shared-group item.
        let shared = read(key, group: sharedGroup)
        if let value = shared.value {
            return value
        }
        if shared.status != errSecItemNotFound {
            logger.error("get(\(key, privacy: .public)) shared-group read failed: \(shared.status)")
            throw KeychainReadError(status: shared.status)
        }
        // Legacy fallback: an item written before the shared group existed.
        let legacy = read(key, group: legacyGroup)
        if let value = legacy.value {
            // Promote it into the shared group so the extension can see it next
            // time. set() clears the legacy copy only on a CONFIRMED write.
            set(key, value: value)
            return value
        }
        if legacy.status != errSecItemNotFound {
            logger.error("get(\(key, privacy: .public)) legacy read failed: \(legacy.status)")
            throw KeychainReadError(status: legacy.status)
        }
        return nil
    }

    @discardableResult
    public func set(_ key: String, value: String?) -> Bool {
        guard let value, let data = value.data(using: .utf8) else {
            delete(key)
            return true
        }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecAttrAccessGroup as String: sharedGroup,
        ]

        // Update-or-add against the shared-group item. Never delete first: a
        // delete-then-add loses the stored value the moment the add fails.
        var status = SecItemUpdate(query as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if status == errSecItemNotFound {
            var addQuery = query
            addQuery[kSecValueData as String] = data
            addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
            status = SecItemAdd(addQuery as CFDictionary, nil)
        }
        guard status == errSecSuccess else {
            logger.error("set(\(key, privacy: .public)) write failed: \(status)")
            return false
        }
        // Only now is it safe to drop the legacy copy — and ONLY from the
        // legacy group, never via an ungrouped (all-groups) query.
        deleteItem(key, group: legacyGroup)
        return true
    }

    public func delete(_ key: String) {
        deleteItem(key, group: sharedGroup)
        deleteItem(key, group: legacyGroup)
    }

    // MARK: - Private

    private func read(_ key: String, group: String) -> (status: OSStatus, value: String?) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecAttrAccessGroup as String: group,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess, let data = result as? Data else { return (status, nil) }
        return (status, String(data: data, encoding: .utf8))
    }

    private func deleteItem(_ key: String, group: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecAttrAccessGroup as String: group,
        ]
        let status = SecItemDelete(query as CFDictionary)
        if status != errSecSuccess && status != errSecItemNotFound {
            logger.error("delete(\(key, privacy: .public)) in \(group, privacy: .public) failed: \(status)")
        }
    }
}
