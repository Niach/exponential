import ExpPush
import Foundation
import XCTest

// EXP-1435: the sealed push format v1, replayed through eNet's push-only
// UniFFI binding against `packages/domain-contract/fixtures/push-seal.json`
// (the same vectors the Rust core, the web sealer and Android replay). Read
// through `#filePath`: the unit-test bundle carries no repo resources.
final class PushSealFixtureTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpPush/Tests/
            .deletingLastPathComponent()          // ExpPush/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/push-seal.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func hex(_ raw: Any?) throws -> Data {
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

    private func requireBinding() throws {
        #if !ENET_PUSH
        throw XCTSkip("the push-only ExponentialNetFFI.xcframework was not built (ENET_PUSH_ONLY=1 build-ios.sh)")
        #endif
    }

    #if ENET_PUSH
    func testVectorOpensToPlaintextAndParsesToOpened() throws {
        let f = try fixture()
        let recipient = try XCTUnwrap(f["recipient"] as? [String: Any])
        let vector = try XCTUnwrap(f["vector"] as? [String: Any])
        let plaintext = try pushOpen(recipientSecret: hex(recipient["secret"]), sealed: hex(vector["sealed"]))
        XCTAssertEqual(String(data: plaintext, encoding: .utf8), vector["plaintext"] as? String)

        let parsed = try XCTUnwrap(try JSONSerialization.jsonObject(with: plaintext) as? NSDictionary)
        let opened = try XCTUnwrap(vector["opened"] as? NSDictionary)
        XCTAssertEqual(parsed, opened)
    }

    func testVectorOpensThroughTheUserInfoPath() throws {
        let f = try fixture()
        let recipient = try XCTUnwrap(f["recipient"] as? [String: Any])
        let vector = try XCTUnwrap(f["vector"] as? [String: Any])
        let opened = try XCTUnwrap(vector["opened"] as? [String: Any])
        let userInfo: [AnyHashable: Any] = [
            "aps": ["alert": ["title": "New message"], "mutable-content": 1],
            "s": Base64Url.encode(try hex(vector["sealed"])),
        ]
        let result = try XCTUnwrap(SealedPush.open(userInfo: userInfo, secret: try hex(recipient["secret"])))
        XCTAssertEqual(result.title, opened["title"] as? String)
        XCTAssertEqual(result.body, opened["body"] as? String)
        for (key, value) in try XCTUnwrap(opened["data"] as? [String: String]) {
            XCTAssertEqual(result.userInfo[key] as? String, value)
        }
        XCTAssertNil(result.userInfo["s"])
    }

    func testRustSealedOpens() throws {
        let f = try fixture()
        let recipient = try XCTUnwrap(f["recipient"] as? [String: Any])
        let rust = try XCTUnwrap(f["rustSealed"] as? [String: Any])
        let plaintext = try pushOpen(recipientSecret: hex(recipient["secret"]), sealed: hex(rust["sealed"]))
        XCTAssertEqual(String(data: plaintext, encoding: .utf8), rust["plaintext"] as? String)
    }

    func testDeriveFromIdentity() throws {
        let f = try fixture()
        let derive = try XCTUnwrap(f["derive"] as? [String: Any])
        let pair = try pushKeypairDerive(identity: hex(derive["identity"]))
        XCTAssertEqual(pair.secret, try hex(derive["secret"]))
        XCTAssertEqual(pair.public, try hex(derive["public"]))
    }

    func testEveryRefusedCaseThrows() throws {
        let f = try fixture()
        let refused = try XCTUnwrap(f["refused"] as? [[String: Any]])
        XCTAssertFalse(refused.isEmpty)
        for item in refused {
            let name = item["name"] as? String ?? "?"
            XCTAssertThrowsError(
                try pushOpen(recipientSecret: hex(item["secret"]), sealed: hex(item["sealed"])),
                "refused case \"\(name)\" opened"
            )
            // The NSE path drops it: nothing opens.
            let userInfo: [AnyHashable: Any] = ["s": Base64Url.encode(try hex(item["sealed"]))]
            XCTAssertNil(SealedPush.open(userInfo: userInfo, secret: try hex(item["secret"])), name)
        }
    }

    func testSealingToALowOrderPublicThrows() throws {
        let f = try fixture()
        XCTAssertThrowsError(try pushSeal(recipientPublic: hex(f["lowOrderPublic"]), plaintext: Data("x".utf8)))
    }

    func testRecipientPublicMatchesAGeneratedRoundTrip() throws {
        let f = try fixture()
        let recipient = try XCTUnwrap(f["recipient"] as? [String: Any])
        let sealed = try pushSeal(recipientPublic: hex(recipient["public"]), plaintext: Data("round trip".utf8))
        let opened = try pushOpen(recipientSecret: hex(recipient["secret"]), sealed: sealed)
        XCTAssertEqual(String(data: opened, encoding: .utf8), "round trip")

        let fresh = try XCTUnwrap(PushCrypto.generateKeypair())
        XCTAssertEqual(fresh.secret.count, 32)
        XCTAssertEqual(fresh.publicKey.count, 32)
        XCTAssertEqual(fresh.publicKeyBase64Url.count, 43)
    }
    #else
    func testBindingAbsent() throws {
        try requireBinding()
    }
    #endif
}
