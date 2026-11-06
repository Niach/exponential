// #VAPP-268: the runner's Vault (runner.json `storage`: the platform
// keystore) = the Keychain, one generic-password item per key under the
// service = the config id; the device uuid of the `local` identity lives
// under `storage.deviceKey`, created once.
import Foundation
import Security
import ExponentialEngine

/// The Keychain as the Engine's `Vault` (grants, the last screen, the device uuid).
public final class KeychainVault: Vault, @unchecked Sendable {
    let service: String

    public init(service: String) { self.service = service }

    private func query(_ key: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: key]
    }

    public func read(key: String) -> String? {
        var q = query(key)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(decoding: data, as: UTF8.self)
    }

    public func write(key: String, value: String) throws {
        let data = Data(value.utf8)
        let status = SecItemUpdate(query(key) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if status == errSecItemNotFound {
            var q = query(key)
            q[kSecValueData as String] = data
            q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            let added = SecItemAdd(q as CFDictionary, nil)
            guard added == errSecSuccess else { throw EngineError.Failure(code: "VAULT", reason: "keychain add \(added)") }
        } else if status != errSecSuccess {
            throw EngineError.Failure(code: "VAULT", reason: "keychain update \(status)")
        }
    }

    public func delete(key: String) throws {
        let status = SecItemDelete(query(key) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw EngineError.Failure(code: "VAULT", reason: "keychain delete \(status)") }
    }
}

/// A Vault in memory (tests, previews).
public final class MemoryVault: Vault, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String: String] = [:]

    public init(_ values: [String: String] = [:]) { self.values = values }

    public func read(key: String) -> String? { lock.withLock { values[key] } }
    public func write(key: String, value: String) throws { lock.withLock { values[key] = value } }
    public func delete(key: String) throws { lock.withLock { values[key] = nil } }
}

public enum RunnerDevice {
    /// runner.json `storage.deviceKey`.
    public static let key = "eengine/runner/device"

    /// The `local` identity's device uuid: the stored one, else a new one stored once.
    public static func id(_ vault: Vault) -> String {
        if let id = vault.read(key: key), !id.isEmpty { return id }
        let id = UUID().uuidString.lowercased()
        try? vault.write(key: key, value: id)
        return id
    }
}
