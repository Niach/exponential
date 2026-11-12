@testable import ExpCore
import XCTest

/// #PERF-9 STORED TAIL (fixtures/steer-enet.json): the codec, the sealing
/// rule (a keyed tree is never stored in clear) and the file store.
final class SteerStoredTailTests: XCTestCase {
    /// Seals by wrapping (`exp-sealed:v1:<epoch>:<text>`), opens with a key.
    private struct FakeCrypto: SealedCrypto {
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

    private let key = TreeSealKey(publicKey: "pub", epoch: 3)

    private func frames(_ n: Int) -> [String] {
        (0..<n).map { "{\"t\":\"activity\",\"seq\":\($0),\"event\":{\"kind\":\"narration\",\"text\":\"row \($0)\"}}" }
    }

    func testTheBodyRoundTrips() throws {
        let body = try XCTUnwrap(SteerStoredTail.body(frames(5), key: nil, crypto: FakeCrypto()))
        XCTAssertEqual(SteerStoredTail.open(body, keyed: false, keys: [:], crypto: FakeCrypto()), frames(5))
    }

    func testAKeyedTreeIsStoredSealedNeverInClear() throws {
        let body = try XCTUnwrap(SteerStoredTail.body(frames(3), key: key, crypto: FakeCrypto()))
        XCTAssertTrue(body.hasPrefix("exp-sealed:v1:3:"))
        XCTAssertEqual(SteerStoredTail.open(body, keyed: true, keys: [3: Data(count: 32)], crypto: FakeCrypto()), frames(3))
        // Can't seal = nothing to store.
        XCTAssertNil(SteerStoredTail.body(frames(3), key: key, crypto: FakeCrypto(canSeal: false)))
    }

    func testATailThatDoesNotOpenIsNotShown() throws {
        let body = try XCTUnwrap(SteerStoredTail.body(frames(3), key: key, crypto: FakeCrypto()))
        XCTAssertNil(SteerStoredTail.open(body, keyed: true, keys: [:], crypto: FakeCrypto()), "no key here yet")
        XCTAssertNil(SteerStoredTail.open("{not json", keyed: false, keys: [:], crypto: FakeCrypto()), "malformed")
        XCTAssertNil(SteerStoredTail.open("{\"v\":2,\"frames\":[]}", keyed: false, keys: [:], crypto: FakeCrypto()), "unknown version")
        let plain = SteerStoredTail.encode(frames(3))
        XCTAssertNil(SteerStoredTail.open(plain, keyed: true, keys: [3: Data(count: 32)], crypto: FakeCrypto()), "a keyed tree's tail in clear")
        XCTAssertEqual(SteerStoredTail.open(plain, keyed: false, keys: [:], crypto: FakeCrypto()), frames(3))
    }

    func testTheStoreWritesReadsAndForgetsAnAccount() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("tails-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SteerStoredTailStore(directory: dir)
        store.write(accountId: "acct/1", sessionId: "s1", frames: frames(2), key: key, crypto: FakeCrypto(), generation: 0)
        store.write(accountId: "acct-2", sessionId: "s2", frames: frames(2), key: nil, crypto: FakeCrypto(), generation: 0)
        let sealed = try XCTUnwrap(store.read(accountId: "acct/1", sessionId: "s1"))
        XCTAssertTrue(sealed.hasPrefix("exp-sealed:"))
        XCTAssertEqual(store.read(accountId: "acct-2", sessionId: "s2"), SteerStoredTail.encode(frames(2)))
        // A keyed tree that can't be sealed for drops its old file.
        store.write(accountId: "acct/1", sessionId: "s1", frames: frames(2), key: key, crypto: FakeCrypto(canSeal: false), generation: 0)
        XCTAssertNil(store.read(accountId: "acct/1", sessionId: "s1"))
        store.removeAccounts(["acct-2"])
        XCTAssertNil(store.read(accountId: "acct-2", sessionId: "s2"))
        // Sign-out drops the writes already on their way.
        store.write(accountId: "acct-2", sessionId: "s2", frames: frames(2), key: nil, crypto: FakeCrypto(), generation: 0)
        XCTAssertNil(store.read(accountId: "acct-2", sessionId: "s2"))
        store.write(accountId: "acct-2", sessionId: "s2", frames: frames(2), key: nil, crypto: FakeCrypto(), generation: store.generation(accountId: "acct-2"))
        XCTAssertNotNil(store.read(accountId: "acct-2", sessionId: "s2"))
    }

    func testTheRingKeepsTheNewestRowsAndTheLatestOfEachSlot() {
        var ring = SteerStoredTail.Ring()
        ring.append("turn-0", slotKind: "turn")
        for i in 0..<(3 * SteerEnet.storedTail) {
            ring.append("row-\(i)", slotKind: nil)
            // A working agent's turn between every row.
            ring.append("turn-\(i + 1)", slotKind: "turn")
        }
        ring.append("usage", slotKind: "usage")
        let frames = ring.frames
        XCTAssertEqual(frames.count, SteerEnet.storedTail + 2)
        XCTAssertEqual(frames.first, "row-\(2 * SteerEnet.storedTail)")
        XCTAssertEqual(Array(frames.suffix(3)), ["row-\(3 * SteerEnet.storedTail - 1)", "turn-\(3 * SteerEnet.storedTail)", "usage"])
    }

    func testPruningKeepsTheMostRecentlyOpenedAndDropsTheStale() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("tails-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let now = Date()
        let cap = SteerEnet.storedTailMaxSessions
        // cap + 5 tails used one minute apart (s0 the newest), plus one
        // recent-enough by rank but older than the age limit.
        for i in 0..<(cap + 5) {
            let file = dir.appendingPathComponent("s\(i).tail")
            try Data("x".utf8).write(to: file)
            try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(-60 * Double(i))], ofItemAtPath: file.path)
        }
        let stale = dir.appendingPathComponent("s0.tail")
        try FileManager.default.setAttributes(
            [.modificationDate: now.addingTimeInterval(-Double(SteerEnet.storedTailMaxAgeDays + 1) * 86_400)], ofItemAtPath: stale.path
        )
        SteerStoredTailStore.prune(dir, now: now)
        let left = Set(try FileManager.default.contentsOfDirectory(atPath: dir.path))
        XCTAssertFalse(left.contains("s0.tail"), "not opened within the age limit")
        XCTAssertEqual(left.count, cap, "beyond the cap only the most recently opened stay")
        XCTAssertTrue(left.contains("s1.tail"))
        XCTAssertFalse(left.contains("s\(cap + 1).tail"))
    }

    func testReadingStampsTheLastUse() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("tails-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SteerStoredTailStore(directory: dir)
        store.write(accountId: "a", sessionId: "s", frames: frames(1), key: nil, crypto: FakeCrypto(), generation: 0)
        let file = dir.appendingPathComponent("a/s.tail")
        _ = store.read(accountId: "a", sessionId: "s")
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSinceNow: -86_400)], ofItemAtPath: file.path)
        XCTAssertNotNil(store.read(accountId: "a", sessionId: "s"))
        let stamped = try XCTUnwrap(try file.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate)
        XCTAssertLessThan(Date().timeIntervalSince(stamped), 60)
    }

    func testSlotKinds() {
        XCTAssertEqual(SteerStoredTail.slotKind(.diff(nil)), "diff")
        XCTAssertEqual(SteerStoredTail.slotKind(.queue([])), "queue")
        XCTAssertNil(SteerStoredTail.slotKind(.userMessage(text: "hi", subagentId: nil)))
    }
}
