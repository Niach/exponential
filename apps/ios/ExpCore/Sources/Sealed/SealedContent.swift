import Foundation

// EXP-1459: sealed session content on iOS (the wire contract is
// packages/domain-contract/fixtures/sealed-content.json, its $comment the
// spec). Bodies, cards, sealed titles and digest previews travel as
// envelopes sealed to the session TREE's key; this install opens them with
// the tree secrets it holds (`TreeKeyHolder` fetches them wrapped to its
// install key and keeps them in the shared keychain). A body without the
// `exp-sealed:` prefix is plaintext and stays legal.

/// Why a message is locked (sealed-content.json `placeholder.reasons`).
public enum LockedReason: String, Sendable, Equatable {
    /// No wrap for this install yet: opens by itself once a holder wraps it.
    case noKey
    /// A newer version, malformed or refused: never opens here.
    case unreadable
}

/// THE locked state ×4 (sealed-content.json `placeholder`).
public enum SealedPlaceholder {
    /// The lock concept the transcript pill shows.
    public static let icon = "ui-private"
    /// The locked pill's test id (web `SEALED_LOCKED_TEST_ID`).
    public static let testId = "sealed-locked"

    public static func copy(_ reason: LockedReason) -> String {
        switch reason {
        case .noKey: "Waiting for the key"
        case .unreadable: "Can't be opened here"
        }
    }

    /// `placeholder.reasons`: plain/opened = not locked.
    public static func reason(_ status: SealStatus) -> LockedReason? {
        switch status {
        case .plain, .opened: nil
        case .noKey: .noKey
        case .unsupported, .malformed, .refused: .unreadable
        }
    }
}

/// What opening sealed content gave (the fixture's spelling).
public enum SealStatus: String, Sendable, Equatable {
    case plain, opened, noKey, unsupported, malformed, refused

    /// The content is readable (`text` / the full card).
    public var isReadable: Bool { self == .plain || self == .opened }
}

public struct OpenedText: Equatable, Sendable {
    public let status: SealStatus
    public let epoch: UInt32?
    /// Set for plain and opened only.
    public let text: String?

    public init(status: SealStatus, epoch: UInt32?, text: String?) {
        self.status = status
        self.epoch = epoch
        self.text = text
    }

    public static func plain(_ text: String) -> OpenedText { OpenedText(status: .plain, epoch: nil, text: text) }
}

public struct OpenedCardJson: Equatable, Sendable {
    public let status: SealStatus
    public let epoch: UInt32?
    /// The full card (plain / opened) or the clear part alone.
    public let json: String

    public init(status: SealStatus, epoch: UInt32?, json: String) {
        self.status = status
        self.epoch = epoch
        self.json = json
    }
}

public struct GeneratedTreeKey: Sendable {
    public let secret: Data
    public let publicKey: String

    public init(secret: Data, publicKey: String) {
        self.secret = secret
        self.publicKey = publicKey
    }
}

/// eNet's sealed-content functions (the binding; a fake in tests).
public protocol SealedCrypto: Sendable {
    /// False without the binding: nothing opens and nothing is sealed.
    var isAvailable: Bool { get }
    /// The install's X25519 keypair, derived from its identity
    /// (`PushKeypair::derive`; public = base64url, as `installs.x25519_public`).
    func installKey(identity: Data) throws -> (secret: Data, publicKey: String)
    func generateTreeKey() -> GeneratedTreeKey
    func wrap(installPublic: String, treeSecret: Data) throws -> String
    func unwrap(installSecret: Data, wrapped: String) throws -> Data
    func sealBody(treePublicKey: String, epoch: UInt32, text: String) throws -> String
    func openBody(_ body: String, keys: [UInt32: Data]) -> OpenedText
    func sealCard(treePublicKey: String, epoch: UInt32, cardJson: String) throws -> String
    func openCard(_ cardJson: String, keys: [UInt32: Data]) -> OpenedCardJson
}

public enum SealedEnvelope {
    public static let bodyPrefix = "exp-sealed:"

    public static func isSealedBody(_ body: String) -> Bool { body.hasPrefix(bodyPrefix) }

    /// A card JSON that MAY carry a sealed part (cheap pre-check before the FFI).
    public static func mayBeSealedCard(_ json: String) -> Bool { json.contains("\"sealed\"") }
}

public enum SealedCryptoError: Error, Equatable {
    case unavailable
}

/// The crypto when this build does not link eNet: plaintext passes, sealed
/// content stays locked, writes refuse (the caller sends plaintext).
public struct UnavailableSealedCrypto: SealedCrypto {
    public init() {}
    public var isAvailable: Bool { false }
    public func installKey(identity: Data) throws -> (secret: Data, publicKey: String) { throw SealedCryptoError.unavailable }
    public func generateTreeKey() -> GeneratedTreeKey { GeneratedTreeKey(secret: Data(), publicKey: "") }
    public func wrap(installPublic: String, treeSecret: Data) throws -> String { throw SealedCryptoError.unavailable }
    public func unwrap(installSecret: Data, wrapped: String) throws -> Data { throw SealedCryptoError.unavailable }
    public func sealBody(treePublicKey: String, epoch: UInt32, text: String) throws -> String { throw SealedCryptoError.unavailable }
    public func openBody(_ body: String, keys: [UInt32: Data]) -> OpenedText {
        SealedEnvelope.isSealedBody(body) ? OpenedText(status: .noKey, epoch: nil, text: nil) : .plain(body)
    }
    public func sealCard(treePublicKey: String, epoch: UInt32, cardJson: String) throws -> String { throw SealedCryptoError.unavailable }
    public func openCard(_ cardJson: String, keys: [UInt32: Data]) -> OpenedCardJson {
        guard var object = (try? JSONSerialization.jsonObject(with: Data(cardJson.utf8))) as? [String: Any],
              object["sealed"] != nil else {
            return OpenedCardJson(status: .plain, epoch: nil, json: cardJson)
        }
        object.removeValue(forKey: "sealed")
        let clear = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        return OpenedCardJson(status: .noKey, epoch: nil, json: clear)
    }
}

#if ENET_NODE
/// The full binding's sealed-content functions (exponential-net-ffi).
public struct EnetSealedCrypto: SealedCrypto {
    public init() {}
    public var isAvailable: Bool { true }

    public func installKey(identity: Data) throws -> (secret: Data, publicKey: String) {
        let pair = try pushKeypairDerive(identity: identity)
        return (pair.secret, InstallRegistration.base64Url(pair.public))
    }

    public func generateTreeKey() -> GeneratedTreeKey {
        let key = ExpCore.generateTreeKey()
        return GeneratedTreeKey(secret: key.secret, publicKey: key.publicKey)
    }

    public func wrap(installPublic: String, treeSecret: Data) throws -> String {
        try wrapTreeKey(installPublic: installPublic, treeSecret: treeSecret)
    }

    public func unwrap(installSecret: Data, wrapped: String) throws -> Data {
        try unwrapTreeKey(installSecret: installSecret, wrapped: wrapped)
    }

    public func sealBody(treePublicKey: String, epoch: UInt32, text: String) throws -> String {
        try ExpCore.sealBody(treePublicKey: treePublicKey, epoch: epoch, text: text)
    }

    public func openBody(_ body: String, keys: [UInt32: Data]) -> OpenedText {
        guard SealedEnvelope.isSealedBody(body) else { return .plain(body) }
        do {
            let opened = try ExpCore.openBody(body: body, keys: Self.epochKeys(keys))
            return OpenedText(status: SealStatus(opened.status), epoch: opened.epoch, text: opened.text)
        } catch {
            return OpenedText(status: .malformed, epoch: nil, text: nil)
        }
    }

    public func sealCard(treePublicKey: String, epoch: UInt32, cardJson: String) throws -> String {
        try ExpCore.sealCard(treePublicKey: treePublicKey, epoch: epoch, cardJson: cardJson)
    }

    public func openCard(_ cardJson: String, keys: [UInt32: Data]) -> OpenedCardJson {
        guard SealedEnvelope.mayBeSealedCard(cardJson) else {
            return OpenedCardJson(status: .plain, epoch: nil, json: cardJson)
        }
        do {
            let opened = try ExpCore.openCard(cardJson: cardJson, keys: Self.epochKeys(keys))
            return OpenedCardJson(status: SealStatus(opened.status), epoch: opened.epoch, json: opened.cardJson)
        } catch {
            return UnavailableSealedCrypto().openCard(cardJson, keys: [:]).withStatus(.malformed)
        }
    }

    private static func epochKeys(_ keys: [UInt32: Data]) -> [EnetEpochKey] {
        keys.filter { $0.value.count == 32 }.map { EnetEpochKey(epoch: $0.key, secret: $0.value) }
    }
}

extension SealStatus {
    init(_ status: EnetSealStatus) {
        switch status {
        case .plain: self = .plain
        case .opened: self = .opened
        case .noKey: self = .noKey
        case .unsupported: self = .unsupported
        case .malformed: self = .malformed
        case .refused: self = .refused
        }
    }
}
#endif

extension OpenedCardJson {
    func withStatus(_ status: SealStatus) -> OpenedCardJson { OpenedCardJson(status: status, epoch: epoch, json: json) }
}

/// The crypto this build links.
public enum SealedCryptoFactory {
    public static func make() -> SealedCrypto {
        #if ENET_NODE
        EnetSealedCrypto()
        #else
        UnavailableSealedCrypto()
        #endif
    }
}
