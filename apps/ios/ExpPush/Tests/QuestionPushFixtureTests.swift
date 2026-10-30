import ExpPush
import Foundation
import XCTest

// #VAPP-45: question-push.json replayed through the Notification Service
// Extension's pieces: `textCases` (the ask preview, clipped) everywhere,
// `bodyCases` (open + fallback) through the push-only binding.
final class QuestionPushFixtureTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/question-push.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func hex(_ string: String) throws -> Data {
        var bytes = [UInt8]()
        var index = string.startIndex
        while index < string.endIndex {
            let next = string.index(index, offsetBy: 2)
            bytes.append(try XCTUnwrap(UInt8(string[index..<next], radix: 16)))
            index = next
        }
        return Data(bytes)
    }

    func testConstants() throws {
        let f = try fixture()
        XCTAssertEqual(f["bodyMax"] as? Int, QuestionPush.bodyMax)
        let keys = try XCTUnwrap(f["dataKeys"] as? [String: String])
        XCTAssertEqual(keys["sealedText"], QuestionPush.sealedTextKey)
        XCTAssertEqual(keys["rootId"], QuestionPush.rootIdKey)
    }

    func testTextCases() throws {
        for case let c as [String: Any] in try XCTUnwrap(fixture()["textCases"] as? [Any]) {
            XCTAssertEqual(QuestionPush.text(askText: c["askText"] as! String), c["shown"] as? String, c["name"] as? String ?? "")
        }
    }

    func testBodyCases() throws {
        for case let c as [String: Any] in try XCTUnwrap(fixture()["bodyCases"] as? [Any]) {
            let name = c["name"] as? String ?? ""
            let keys = try (c["keys"] as? [String: String]).map { raw in
                try Dictionary(uniqueKeysWithValues: raw.map { (UInt32($0.key)!, try hex($0.value)) })
            }
            let data = try XCTUnwrap(c["data"] as? [String: Any])
            let shown = QuestionPush.body(
                data: data,
                keysFor: { $0 == data[QuestionPush.rootIdKey] as? String ? keys : nil },
                fallback: c["fallback"] as! String
            )
            #if ENET_PUSH
            XCTAssertEqual(shown, c["shown"] as? String, name)
            #else
            // Without the binding nothing opens: every case keeps the push body.
            XCTAssertEqual(shown, c["fallback"] as? String, name)
            #endif
        }
    }

    func testTreeKeyStoreKeysByRoot() {
        let store = TreeKeyStore(storage: MemoryTreeKeySecretStorage())
        store.save(rootId: "ABC", epoch: 1, secret: Data(repeating: 1, count: 32))
        XCTAssertEqual(store.keys(rootId: "Abc"), [1: Data(repeating: 1, count: 32)])
        XCTAssertNil(store.keys(rootId: "other"))
    }
}
