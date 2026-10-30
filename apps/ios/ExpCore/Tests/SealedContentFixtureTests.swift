@testable import ExpCore
import Foundation
import XCTest

// EXP-1459: packages/domain-contract/fixtures/sealed-content.json replayed
// through the FULL eNet binding (ExpCore's `EnetSealedCrypto`), the same
// vectors TS, Rust and Kotlin replay. ExpPushTests replays them through the
// push-only binding the Notification Service Extension links.
final class SealedContentFixtureTests: XCTestCase {
    static func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/sealed-content.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    static func hex(_ raw: Any?) throws -> Data {
        let string = try XCTUnwrap(raw as? String)
        var bytes = [UInt8]()
        var index = string.startIndex
        while index < string.endIndex {
            let next = string.index(index, offsetBy: 2)
            bytes.append(try XCTUnwrap(UInt8(string[index..<next], radix: 16)))
            index = next
        }
        return Data(bytes)
    }

    static func keys(_ f: [String: Any]) throws -> [UInt32: Data] {
        let raw = try XCTUnwrap(f["keys"] as? [String: Any])
        var out: [UInt32: Data] = [:]
        for (epoch, secret) in raw { out[try XCTUnwrap(UInt32(epoch))] = try hex(secret) }
        return out
    }

    static func json(_ object: Any) throws -> String {
        String(decoding: try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]), as: UTF8.self)
    }

    static func object(_ json: String) throws -> NSDictionary {
        try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(json.utf8)) as? NSDictionary)
    }

    private func crypto() throws -> SealedCrypto {
        #if ENET_NODE
        return EnetSealedCrypto()
        #else
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
        #endif
    }

    func testWrapVectorUnwrapsAndRoundTrips() throws {
        let crypto = try crypto()
        let f = try Self.fixture()
        let install = try XCTUnwrap(f["install"] as? [String: Any])
        let wrap = try XCTUnwrap(f["wrap"] as? [String: Any])
        let treeSecret = try Self.hex(wrap["treeSecret"])
        let installSecret = try Self.hex(install["secret"])
        XCTAssertEqual(try crypto.unwrap(installSecret: installSecret, wrapped: try XCTUnwrap(wrap["wrapped"] as? String)), treeSecret)
        let fresh = try crypto.wrap(installPublic: try XCTUnwrap(wrap["installPublic"] as? String), treeSecret: treeSecret)
        XCTAssertEqual(fresh.count, 108)
        XCTAssertEqual(try crypto.unwrap(installSecret: installSecret, wrapped: fresh), treeSecret)
        // A wrap for someone else never opens here.
        let other = try Self.hex(try XCTUnwrap(f["rotatedTree"] as? [String: Any])["secret"])
        XCTAssertThrowsError(try crypto.unwrap(installSecret: other, wrapped: fresh))
    }

    func testBodyVectorOpensAndSealsRoundTrip() throws {
        let crypto = try crypto()
        let f = try Self.fixture()
        let body = try XCTUnwrap(f["body"] as? [String: Any])
        let keys = try Self.keys(f)
        let text = try XCTUnwrap(body["text"] as? String)
        let opened = crypto.openBody(try XCTUnwrap(body["envelope"] as? String), keys: keys)
        XCTAssertEqual(opened, OpenedText(status: .opened, epoch: 1, text: text))
        let sealed = try crypto.sealBody(treePublicKey: try XCTUnwrap(body["treePublicKey"] as? String), epoch: 1, text: text)
        XCTAssertTrue(sealed.hasPrefix("exp-sealed:v1:1:"))
        XCTAssertEqual(crypto.openBody(sealed, keys: keys).text, text)
    }

    func testBodyCases() throws {
        let crypto = try crypto()
        let f = try Self.fixture()
        let keys = try Self.keys(f)
        for case let c as [String: Any] in try XCTUnwrap(f["bodyCases"] as? [Any]) {
            let name = c["name"] as? String ?? "?"
            let opened = crypto.openBody(try XCTUnwrap(c["body"] as? String), keys: keys)
            XCTAssertEqual(opened.status.rawValue, c["status"] as? String, name)
            XCTAssertEqual(opened.epoch.map(Int.init), c["epoch"] as? Int, name)
            XCTAssertEqual(opened.text, c["text"] as? String, name)
        }
    }

    func testCardSplitsOpenAndReseal() throws {
        let crypto = try crypto()
        let f = try Self.fixture()
        let keys = try Self.keys(f)
        let card = try XCTUnwrap(f["card"] as? [String: Any])
        let publicKey = try XCTUnwrap(card["treePublicKey"] as? String)
        for case let split as [String: Any] in try XCTUnwrap(card["splits"] as? [Any]) {
            let name = split["name"] as? String ?? "?"
            let full = try XCTUnwrap(split["card"] as? NSDictionary)
            // `opened` = the sealed card with the clear fields laid over it,
            // except the `pictures` projection (the sealed pictures stay whole).
            let shown = NSMutableDictionary(dictionary: full)
            var over = try XCTUnwrap(split["clear"] as? [AnyHashable: Any])
            over["pictures"] = nil
            shown.addEntries(from: over)
            let opened = crypto.openCard(try Self.json(try XCTUnwrap(split["envelope"])), keys: keys)
            XCTAssertEqual(opened.status, .opened, name)
            XCTAssertEqual(try Self.object(opened.json), shown, name)
            // Our own seal: the clear part is exactly the fixture's, and it opens.
            let sealed = try Self.object(try crypto.sealCard(treePublicKey: publicKey, epoch: 1, cardJson: try Self.json(full)))
            let clear = NSMutableDictionary(dictionary: sealed)
            clear.removeObject(forKey: "sealed")
            XCTAssertEqual(clear, split["clear"] as? NSDictionary, name)
            XCTAssertEqual(try Self.object(crypto.openCard(try Self.json(sealed), keys: keys).json), shown, name)
        }
    }

    func testCardCases() throws {
        let crypto = try crypto()
        let f = try Self.fixture()
        let keys = try Self.keys(f)
        for case let c as [String: Any] in try XCTUnwrap(f["cardCases"] as? [Any]) {
            let name = c["name"] as? String ?? "?"
            let opened = crypto.openCard(try Self.json(try XCTUnwrap(c["card"])), keys: keys)
            XCTAssertEqual(opened.status.rawValue, c["status"] as? String, name)
            XCTAssertEqual(opened.epoch.map(Int.init), c["epoch"] as? Int, name)
            XCTAssertEqual(try Self.object(opened.json), c["opened"] as? NSDictionary, name)
        }
    }

    /// `placeholder.messageCases`: openMessage = the opened body/card, or
    /// the locked reason with an empty body and the card's clear part.
    func testPlaceholderMessageCases() throws {
        let crypto = try crypto()
        let f = try Self.fixture()
        let placeholder = try XCTUnwrap(f["placeholder"] as? [String: Any])
        for c in try XCTUnwrap(placeholder["messageCases"] as? [[String: Any]]) {
            let name = c["name"] as? String ?? "?"
            let ring = SealedKeyRing(persistence: .memory)
            var keys: [(rootId: String, epoch: UInt32, secret: Data)] = []
            for (epoch, secret) in try XCTUnwrap(c["keys"] as? [String: String]) {
                keys.append(("r", try XCTUnwrap(UInt32(epoch)), try Self.hex(secret)))
            }
            ring.add(keys)
            let opener = SealedOpener(crypto: crypto, ring: ring)
            let message = try XCTUnwrap(c["message"] as? [String: Any])
            let card = message["card"] is NSNull ? nil : try (message["card"]).map(Self.json)
            let opened = SealedMessage.open(body: try XCTUnwrap(message["body"] as? String), card: card, rootId: "r", opener: opener)
            let expected = try XCTUnwrap(c["expected"] as? [String: Any])
            XCTAssertEqual(SealedPlaceholder.reason(opened.status)?.rawValue, expected["locked"] as? String, name)
            XCTAssertEqual(opened.body, expected["body"] as? String, name)
            if let want = expected["card"] as? NSDictionary {
                XCTAssertEqual(try Self.object(try XCTUnwrap(opened.card)), want, name)
            } else {
                XCTAssertNil(opened.card, name)
            }
        }
    }

    /// The opener (what every view reads through) gives the same answers,
    /// keyed by root, and asks for a missing root's key.
    func testOpenerOpensByRootAndReportsMissingKeys() throws {
        let crypto = try crypto()
        let f = try Self.fixture()
        let body = try XCTUnwrap(f["body"] as? [String: Any])
        let envelope = try XCTUnwrap(body["envelope"] as? String)
        let ring = SealedKeyRing(persistence: .memory)
        let opener = SealedOpener(crypto: crypto, ring: ring)
        let missing = Locked<[String]>([])
        opener.onMissingKey { root in missing.mutate { $0.append(root) } }

        XCTAssertEqual(opener.body("hello", rootId: "r1"), .plain("hello"))
        XCTAssertEqual(opener.body(envelope, rootId: "r1").status, .noKey)
        XCTAssertEqual(missing.value, ["r1"])

        ring.add([("R1", 1, try Self.hex(try XCTUnwrap(f["tree"] as? [String: Any])["secret"]))])
        XCTAssertEqual(opener.body(envelope, rootId: "r1").text, body["text"] as? String)
        XCTAssertEqual(opener.title(sealedTitle: envelope, title: "Session", rootId: "r1"), body["text"] as? String)
        XCTAssertEqual(opener.title(sealedTitle: envelope, title: "Session", rootId: "r2"), "Session")

        let message = SealedMessage.open(body: envelope, card: nil as String?, rootId: "r2", opener: opener)
        XCTAssertEqual(message.body, "")
        XCTAssertEqual(message.status, .noKey)
    }

    /// An escalated `ask` mirror's `sealedCard` opens under the MIRROR's tree
    /// key and stands in for an origin card this install can't open.
    func testEscalatedAskMirrorOpensUnderItsOwnTree() throws {
        let crypto = try crypto()
        let f = try Self.fixture()
        let card = try XCTUnwrap(f["card"] as? [String: Any])
        let splits = try XCTUnwrap(card["splits"] as? [[String: Any]])
        let question = try XCTUnwrap(splits.first { ($0["card"] as? [String: Any])?["kind"] as? String == "question" })
        let envelope = try XCTUnwrap(question["envelope"])
        let ring = SealedKeyRing(persistence: .memory)
        let opener = SealedOpener(crypto: crypto, ring: ring)
        let mirror = try Self.json(["kind": "ask", "sessionId": "s1", "messageId": "m1", "sealedCard": envelope])

        XCTAssertNil(SealedMessage.escalatedCard(mirror, rootId: "watch", opener: opener))
        ring.add([("watch", 1, try Self.hex(try XCTUnwrap(f["tree"] as? [String: Any])["secret"]))])
        let opened = try XCTUnwrap(TreeMessageCard.parse(SealedMessage.escalatedCard(mirror, rootId: "watch", opener: opener)))
        XCTAssertEqual(opened.kind, "question")
        XCTAssertEqual(opened.question, (question["card"] as? [String: Any])?["question"] as? String)
        // Not an ask, or no sealedCard: nothing.
        XCTAssertNil(SealedMessage.escalatedCard(try Self.json(["kind": "ask", "messageId": "m1"]), rootId: "watch", opener: opener))

        let clear = try XCTUnwrap(TreeMessageCard.parse(try Self.json(try XCTUnwrap(question["clear"]))))
        let origin = TreeMessage(
            id: "m1", sessionId: "s1", authorKind: "agent", authorId: nil, body: "",
            card: clear, answeredAt: nil, consumedAt: nil, createdAt: "2026-10-30T00:00:00.000Z",
            sealStatus: .noKey
        )
        XCTAssertEqual(origin.opened(escalated: opened).card, opened)
        XCTAssertEqual(origin.opened(escalated: opened).sealStatus, .opened)
        XCTAssertEqual(origin.opened(escalated: nil), origin)
    }
}

/// A tiny lock box for test callbacks.
final class Locked<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var _value: T
    init(_ value: T) { _value = value }
    var value: T { lock.withLock { _value } }
    func mutate(_ body: (inout T) -> Void) { lock.withLock { body(&_value) } }
}
