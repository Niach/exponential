@testable import ExpCore
import XCTest

/// Seals by wrapping (`exp-sealed:v1:<epoch>:<text>`), opens with a key.
struct TailFakeCrypto: SealedCrypto {
    var canSeal = true
    var isAvailable: Bool { true }
    func installKey(identity: Data) throws -> (secret: Data, publicKey: String) { (Data(), "") }
    func generateTreeKey() -> GeneratedTreeKey { GeneratedTreeKey(secret: Data(), publicKey: "") }
    func wrap(installPublic: String, treeSecret: Data) throws -> String { "" }
    func unwrap(installSecret: Data, wrapped: String) throws -> Data { Data() }
    func sealBody(treePublicKey: String, epoch: UInt32, text: String) throws -> String {
        guard canSeal else { throw SealedCryptoError.unavailable }
        return "exp-sealed:v1:\(epoch):\(text)"
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

/// #PERF-9 STORED TAIL (fixtures/steer-enet.json): the codec, the sealing
/// rule (a keyed tree is never stored in clear) and the file store, read-only
/// since #EDE-114 (the chat store reads every old tail in once).
final class SteerStoredTailTests: XCTestCase {
    private let key = TreeSealKey(publicKey: "pub", epoch: 3)

    private func frames(_ n: Int) -> [String] {
        (0..<n).map { "{\"t\":\"activity\",\"seq\":\($0),\"event\":{\"kind\":\"narration\",\"text\":\"row \($0)\"}}" }
    }

    func testTheBodyRoundTrips() throws {
        let body = try XCTUnwrap(SteerStoredTail.body(frames(5), key: nil, crypto: TailFakeCrypto()))
        XCTAssertEqual(SteerStoredTail.open(body, keyed: false, keys: [:], crypto: TailFakeCrypto()), frames(5))
    }

    func testAKeyedTreeIsStoredSealedNeverInClear() throws {
        let body = try XCTUnwrap(SteerStoredTail.body(frames(3), key: key, crypto: TailFakeCrypto()))
        XCTAssertTrue(body.hasPrefix("exp-sealed:v1:3:"))
        XCTAssertEqual(SteerStoredTail.open(body, keyed: true, keys: [3: Data(count: 32)], crypto: TailFakeCrypto()), frames(3))
        // Can't seal = nothing to store.
        XCTAssertNil(SteerStoredTail.body(frames(3), key: key, crypto: TailFakeCrypto(canSeal: false)))
    }

    func testATailThatDoesNotOpenIsNotShown() throws {
        let body = try XCTUnwrap(SteerStoredTail.body(frames(3), key: key, crypto: TailFakeCrypto()))
        XCTAssertNil(SteerStoredTail.open(body, keyed: true, keys: [:], crypto: TailFakeCrypto()), "no key here yet")
        XCTAssertNil(SteerStoredTail.open("{not json", keyed: false, keys: [:], crypto: TailFakeCrypto()), "malformed")
        XCTAssertNil(SteerStoredTail.open("{\"v\":2,\"frames\":[]}", keyed: false, keys: [:], crypto: TailFakeCrypto()), "unknown version")
        let plain = SteerStoredTail.encode(frames(3))
        XCTAssertNil(SteerStoredTail.open(plain, keyed: true, keys: [3: Data(count: 32)], crypto: TailFakeCrypto()), "a keyed tree's tail in clear")
        XCTAssertEqual(SteerStoredTail.open(plain, keyed: false, keys: [:], crypto: TailFakeCrypto()), frames(3))
    }

    /// #EXP-1853: a Swift Dictionary seeds its order per instance (its
    /// storage address), so the same frames must still encode to one text
    /// however many dictionaries are alive around them.
    func testTheEncodingIsCanonical() {
        var alive: [[String: Any]] = []
        let texts = Set((0..<500).map { i -> String in
            alive.append(["v": i, "frames": frames(i % 3)])
            return SteerStoredTail.encode(frames(2))
        })
        XCTAssertEqual(texts, [#"{"frames":["{\"t\":\"activity\",\"seq\":0,\"event\":{\"kind\":\"narration\",\"text\":\"row 0\"}}","{\"t\":\"activity\",\"seq\":1,\"event\":{\"kind\":\"narration\",\"text\":\"row 1\"}}"],"v":1}"#])
        XCTAssertEqual(alive.count, 500)
    }

    func testTheStoreReadsListsAndForgetsTails() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("tails-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SteerStoredTailStore(directory: dir)
        let sealed = try XCTUnwrap(SteerStoredTail.body(frames(2), key: key, crypto: TailFakeCrypto()))
        try store.writeBody(accountId: "acct-1", sessionId: "s1", body: sealed)
        try store.writeBody(accountId: "acct-1", sessionId: "s0", body: SteerStoredTail.encode(frames(1)))
        try store.writeBody(accountId: "acct-2", sessionId: "s2", body: SteerStoredTail.encode(frames(2)))
        XCTAssertEqual(store.sessionIds(accountId: "acct-1"), ["s0", "s1"])
        XCTAssertEqual(store.read(accountId: "acct-1", sessionId: "s1"), sealed)
        store.remove(accountId: "acct-1", sessionId: "s1")
        XCTAssertNil(store.read(accountId: "acct-1", sessionId: "s1"))
        store.removeAccounts(["acct-2"])
        XCTAssertNil(store.read(accountId: "acct-2", sessionId: "s2"))
        XCTAssertEqual(store.sessionIds(accountId: "acct-2"), [])
        XCTAssertEqual(store.sessionIds(accountId: "acct-1"), ["s0"])
    }
}
