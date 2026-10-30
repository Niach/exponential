import Foundation
import Security

/// The device's push keypair in the keychain access group the app and its
/// extensions share (`<team>.at.exponential.shared`, staging
/// `<team>.at.exponential.staging.shared`), readable after the first unlock so
/// the Notification Service Extension opens pushes on a locked phone. One
/// keypair per install, never synced off the device.
public final class PushKeyStore: Sendable {
    /// Apple team prefix (`SharedAppGroup.teamPrefix` in ExpCore; ExpPush stays
    /// Foundation + Security only, so it is repeated here).
    private static let teamPrefix = "V6W7BVCSM8"
    private let service = "at.exponential.push"
    private let account = "x25519.v1"
    private let group: String

    public init(bundleIdentifier: String? = Bundle.main.bundleIdentifier) {
        // Staging is detected from the host bundle id, like SharedAppGroup: the
        // staging app and its extensions all carry `.staging`.
        let staging = bundleIdentifier?.contains(".staging") ?? false
        group = staging
            ? "\(Self.teamPrefix).at.exponential.staging.shared"
            : "\(Self.teamPrefix).at.exponential.shared"
    }

    /// The stored keypair, or nil (absent, unreadable, or malformed).
    public func load() -> PushKeypair? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrAccessGroup as String: group,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data, data.count == 64 else { return nil }
        return PushKeypair(secret: Data(data.prefix(32)), publicKey: Data(data.suffix(32)))
    }

    /// The stored keypair, generating and storing one when there is none.
    /// nil when the binding is absent or the keychain refuses the write (an
    /// unsigned simulator build has no access group): the caller registers
    /// legacy.
    public func loadOrCreate() -> PushKeypair? {
        if let existing = load() { return existing }
        guard let fresh = PushCrypto.generateKeypair() else { return nil }
        var item: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrAccessGroup as String: group,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        item[kSecValueData as String] = fresh.secret + fresh.publicKey
        let status = SecItemAdd(item as CFDictionary, nil)
        if status == errSecDuplicateItem { return load() }
        return status == errSecSuccess ? fresh : nil
    }
}
