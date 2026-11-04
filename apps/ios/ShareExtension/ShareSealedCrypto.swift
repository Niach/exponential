import ExpPush
import Foundation

/// EXP-1635: `SealedCrypto` over the push binding: the extension only WRITES,
/// so it never opens anything (and without the binding, `isAvailable` false,
/// a share sends as the app would without eNet).
struct ShareSealedCrypto: SealedCrypto {
    var isAvailable: Bool { PushCrypto.isAvailable }

    func installKey(identity: Data) throws -> (secret: Data, publicKey: String) {
        guard let pair = PushCrypto.deriveKeypair(identity: identity) else { throw SealedCryptoError.unavailable }
        return (pair.secret, pair.publicKeyBase64Url)
    }

    func generateTreeKey() -> GeneratedTreeKey {
        guard let key = PushCrypto.generateTree() else { return GeneratedTreeKey(secret: Data(), publicKey: "") }
        return GeneratedTreeKey(secret: key.secret, publicKey: key.publicKey)
    }

    func wrap(installPublic: String, treeSecret: Data) throws -> String {
        try PushCrypto.wrapTree(installPublic: installPublic, treeSecret: treeSecret)
    }

    func unwrap(installSecret: Data, wrapped: String) throws -> Data {
        try PushCrypto.unwrapTree(installSecret: installSecret, wrapped: wrapped)
    }

    func sealBody(treePublicKey: String, epoch: UInt32, text: String) throws -> String {
        try PushCrypto.sealTreeBody(treePublicKey: treePublicKey, epoch: epoch, text: text)
    }

    func sealCard(treePublicKey: String, epoch: UInt32, cardJson: String) throws -> String {
        throw SealedCryptoError.unavailable
    }

    func openBody(_ body: String, keys: [UInt32: Data]) -> OpenedText {
        UnavailableSealedCrypto().openBody(body, keys: keys)
    }

    func openCard(_ cardJson: String, keys: [UInt32: Data]) -> OpenedCardJson {
        UnavailableSealedCrypto().openCard(cardJson, keys: keys)
    }
}
