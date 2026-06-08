import Foundation
#if !os(macOS)
import Security
#endif

#if os(macOS)

/// macOS credential store backed by a 0600 file in Application Support — NOT the
/// system Keychain.
///
/// The Keychain prompts the user to "allow access" whenever the accessing app's
/// code signature changes, which on macOS happens on *every* debug rebuild
/// (ad-hoc / per-build signatures), so you'd re-enter your login password on
/// each run. That friction isn't worth it for a locally-run desktop app, and no
/// other secret here needs hardware-backed storage. The file lives in the app's
/// Application Support container (user-readable only, 0600). iOS keeps the real
/// Keychain (see the #else branch) — it has no such prompt and needs the shared
/// access group so the Share Extension can read the token.
public final class KeychainStore: Sendable {
    private let lock = NSLock()
    private let url: URL

    public init() {
        let dir = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Exponential", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        self.url = dir.appendingPathComponent("credentials.json")
    }

    public func get(_ key: String) -> String? {
        lock.lock()
        defer { lock.unlock() }
        return load()[key]
    }

    public func set(_ key: String, value: String?) {
        lock.lock()
        defer { lock.unlock() }
        var dict = load()
        if let value {
            dict[key] = value
        } else {
            dict.removeValue(forKey: key)
        }
        save(dict)
    }

    public func delete(_ key: String) {
        lock.lock()
        defer { lock.unlock() }
        var dict = load()
        dict.removeValue(forKey: key)
        save(dict)
    }

    private func load() -> [String: String] {
        guard let data = try? Data(contentsOf: url),
              let dict = try? JSONDecoder().decode([String: String].self, from: data)
        else { return [:] }
        return dict
    }

    private func save(_ dict: [String: String]) {
        guard let data = try? JSONEncoder().encode(dict) else { return }
        try? data.write(to: url, options: .atomic)
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }
}

#else

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

#endif
