@testable import ExpCore
import XCTest

/// #EXP-1954: the viewer's own not-yet-consumed row in a sealed chat draws
/// its opened text, or the locked placeholder, never the `exp-sealed:` envelope.
final class SealedShownBodyTests: XCTestCase {
    /// Seals by wrapping (`exp-sealed:v1:<epoch>:<text>`), opens with a key.
    private struct FakeCrypto: SealedCrypto {
        var isAvailable: Bool { true }
        func installKey(identity: Data) throws -> (secret: Data, publicKey: String) { (Data(), "") }
        func generateTreeKey() -> GeneratedTreeKey { GeneratedTreeKey(secret: Data(), publicKey: "") }
        func wrap(installPublic: String, treeSecret: Data) throws -> String { "" }
        func unwrap(installSecret: Data, wrapped: String) throws -> Data { Data() }
        func sealBody(treePublicKey: String, epoch: UInt32, text: String) throws -> String {
            "exp-sealed:v1:\(epoch):\(text)"
        }
        func openBody(_ body: String, keys: [UInt32: Data]) -> OpenedText {
            guard body.hasPrefix("exp-sealed:v1:") else { return .plain(body) }
            let rest = body.dropFirst("exp-sealed:v1:".count)
            guard let colon = rest.firstIndex(of: ":"), let epoch = UInt32(rest[..<colon]) else {
                return OpenedText(status: .malformed, epoch: nil, text: nil)
            }
            guard keys[epoch] != nil else { return OpenedText(status: .noKey, epoch: epoch, text: nil) }
            return OpenedText(status: .opened, epoch: epoch, text: String(rest[rest.index(after: colon)...]))
        }
        func sealCard(treePublicKey: String, epoch: UInt32, cardJson: String) throws -> String { cardJson }
        func openCard(_ cardJson: String, keys: [UInt32: Data]) -> OpenedCardJson {
            OpenedCardJson(status: .plain, epoch: nil, json: cardJson)
        }
    }

    private func ownRow(body: String) -> MessageEntity {
        MessageEntity(
            id: "m1", conversationId: "c1", rootId: "r1", teamId: "t1", authorKind: "user", authorId: "me",
            body: body, createdAt: "2026-11-18T10:00:00Z", updatedAt: "2026-11-18T10:00:00Z", sessionId: "r1"
        )
    }

    func testASealedOwnRowWithTheKeyShowsItsPlaintext() {
        let ring = SealedKeyRing(persistence: .memory)
        ring.add([("r1", 1, Data(repeating: 1, count: 32))])
        let opener = SealedOpener(crypto: FakeCrypto(), ring: ring)
        let row = ownRow(body: "exp-sealed:v1:1:hello from the phone")
        XCTAssertEqual(SealedMessage.shownBody(row.body, rootId: row.rootId, opener: opener), "hello from the phone")
    }

    func testASealedOwnRowWithoutTheKeyShowsTheLockedPlaceholder() {
        let opener = SealedOpener(crypto: FakeCrypto(), ring: SealedKeyRing(persistence: .memory))
        let row = ownRow(body: "exp-sealed:v1:1:hello from the phone")
        let shown = SealedMessage.shownBody(row.body, rootId: row.rootId, opener: opener)
        XCTAssertEqual(shown, SealedPlaceholder.copy(.noKey))
        XCTAssertFalse(shown.contains("exp-sealed:"))
        let malformed = SealedMessage.shownBody("exp-sealed:v1:x", rootId: row.rootId, opener: opener)
        XCTAssertEqual(malformed, SealedPlaceholder.copy(.unreadable))
    }

    func testAPlainOwnRowPassesUntouched() {
        let opener = SealedOpener(crypto: FakeCrypto(), ring: SealedKeyRing(persistence: .memory))
        XCTAssertEqual(SealedMessage.shownBody("plain", rootId: "r1", opener: opener), "plain")
    }
}
