import Foundation
import Security

/// The device's push keypair in the keychain access group the app and its
/// extensions share (`<team>.at.exponential.shared`, staging
/// `<team>.at.exponential.staging.shared`), readable after the first unlock so
/// the Notification Service Extension opens pushes on a locked phone. One
/// keypair per install, never synced off the device.
///
/// EXP-1457: the keypair is DERIVED from the install's eNet identity
/// (`InstallIdentityStore`, eNet's `PushKeypair::derive`), so the key the push
/// relay seals to is the one the install registry publishes. The extension
/// still reads only the X25519 item, never the identity. When the derived key
/// replaces a different one (the random key of an older build, or a rotated
/// identity), the old one stays as `previous` so pushes sealed before the
/// server learned the new key still open.
public final class PushKeyStore: Sendable {
    /// Apple team prefix (`SharedAppGroup.teamPrefix` in ExpCore; ExpPush stays
    /// Foundation + Security only, so it is repeated here).
    private static let teamPrefix = "V6W7BVCSM8"
    private static let account = "x25519.v1"
    private static let previousAccount = "x25519.prev"
    private let storage: InstallSecretStorage
    private let identity: InstallIdentityStore

    /// The shared keychain group for the host bundle (staging detected from
    /// its id, like SharedAppGroup: the staging app and its extensions all
    /// carry `.staging`).
    public static func sharedGroup(bundleIdentifier: String?) -> String {
        let staging = bundleIdentifier?.contains(".staging") ?? false
        return staging
            ? "\(teamPrefix).at.exponential.staging.shared"
            : "\(teamPrefix).at.exponential.shared"
    }

    public init(storage: InstallSecretStorage, identity: InstallIdentityStore) {
        self.storage = storage
        self.identity = identity
    }

    public convenience init(bundleIdentifier: String? = Bundle.main.bundleIdentifier) {
        self.init(
            storage: KeychainInstallSecretStorage(
                service: "at.exponential.push",
                group: Self.sharedGroup(bundleIdentifier: bundleIdentifier)
            ),
            identity: InstallIdentityStore(bundleIdentifier: bundleIdentifier)
        )
    }

    private func read(_ account: String) -> PushKeypair? {
        guard let data = storage.read(account: account), data.count == 64 else { return nil }
        return PushKeypair(secret: Data(data.prefix(32)), publicKey: Data(data.suffix(32)))
    }

    private func write(_ account: String, _ pair: PushKeypair) -> Bool {
        storage.delete(account: account)
        return storage.add(account: account, data: pair.secret + pair.publicKey)
    }

    /// The stored keypair, or nil (absent, unreadable, or malformed).
    public func load() -> PushKeypair? { read(Self.account) }

    /// Every keypair a push may be sealed to: the current one first, then the
    /// one it replaced. What the Notification Service Extension tries.
    public func loadAll() -> [PushKeypair] {
        [read(Self.account), read(Self.previousAccount)].compactMap { $0 }
    }

    /// The keypair derived from the install identity (created on first use),
    /// stored for the extension. Without an identity (an unsigned simulator
    /// build has no access group) it falls back to a stored or fresh random
    /// keypair; nil when the binding is absent or the keychain refuses the
    /// write: the caller registers legacy.
    public func loadOrCreate() -> PushKeypair? {
        let current = load()
        if let seed = identity.loadOrCreate(), let derived = PushCrypto.deriveKeypair(identity: seed) {
            if current == derived { return derived }
            if let current { _ = write(Self.previousAccount, current) }
            return write(Self.account, derived) ? derived : nil
        }
        if let current { return current }
        guard let fresh = PushCrypto.generateKeypair() else { return nil }
        return write(Self.account, fresh) ? fresh : load()
    }
}
