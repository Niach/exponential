import ExpPush
import Foundation
import XCTest

// EXP-1459: sealed-content.json replayed through the PUSH-ONLY binding (the
// one the Notification Service Extension links, so it can open sealed
// pushes with the tree keys in the shared keychain), plus the keychain
// store's (root, epoch) bookkeeping.
final class SealedContentPushBindingTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/sealed-content.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
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

    func testTreeKeyStoreKeepsSecretsByRootAndEpoch() {
        let store = TreeKeyStore(storage: MemoryTreeKeySecretStorage())
        XCTAssertTrue(store.save(rootId: "ABC", epoch: 1, secret: Data(repeating: 1, count: 32)))
        XCTAssertTrue(store.save(rootId: "abc", epoch: 2, secret: Data(repeating: 2, count: 32)))
        XCTAssertFalse(store.save(rootId: "abc", epoch: 3, secret: Data(repeating: 3, count: 31)))
        XCTAssertEqual(store.load(), ["abc": [1: Data(repeating: 1, count: 32), 2: Data(repeating: 2, count: 32)]])
        store.removeAll()
        XCTAssertEqual(store.load(), [:])
    }

    #if ENET_PUSH
    private func keys(_ f: [String: Any]) throws -> [EnetEpochKey] {
        try XCTUnwrap(f["keys"] as? [String: String]).map { EnetEpochKey(epoch: UInt32($0.key)!, secret: try hex($0.value)) }
    }

    private func json(_ object: Any) throws -> String {
        String(decoding: try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]), as: UTF8.self)
    }

    private func status(_ s: EnetSealStatus) -> String {
        switch s {
        case .plain: "plain"
        case .opened: "opened"
        case .noKey: "noKey"
        case .unsupported: "unsupported"
        case .malformed: "malformed"
        case .refused: "refused"
        }
    }

    func testWrapAndBodyVectors() throws {
        let f = try fixture()
        let install = try XCTUnwrap(f["install"] as? [String: Any])
        let wrap = try XCTUnwrap(f["wrap"] as? [String: Any])
        XCTAssertEqual(try unwrapTreeKey(installSecret: hex(install["secret"]), wrapped: wrap["wrapped"] as! String), try hex(wrap["treeSecret"]))
        let body = try XCTUnwrap(f["body"] as? [String: Any])
        let opened = try openBody(body: body["envelope"] as! String, keys: keys(f))
        XCTAssertEqual(opened.status, .opened)
        XCTAssertEqual(opened.text, body["text"] as? String)
    }

    func testBodyAndCardCases() throws {
        let f = try fixture()
        for case let c as [String: Any] in try XCTUnwrap(f["bodyCases"] as? [Any]) {
            let opened = try openBody(body: c["body"] as! String, keys: keys(f))
            XCTAssertEqual(status(opened.status), c["status"] as? String, c["name"] as? String ?? "")
            XCTAssertEqual(opened.epoch.map(Int.init), c["epoch"] as? Int, c["name"] as? String ?? "")
            XCTAssertEqual(opened.text, c["text"] as? String, c["name"] as? String ?? "")
        }
        for case let c as [String: Any] in try XCTUnwrap(f["cardCases"] as? [Any]) {
            let opened = try openCard(cardJson: json(c["card"]!), keys: keys(f))
            XCTAssertEqual(status(opened.status), c["status"] as? String, c["name"] as? String ?? "")
            let card = try JSONSerialization.jsonObject(with: Data(opened.cardJson.utf8)) as? NSDictionary
            XCTAssertEqual(card, c["opened"] as? NSDictionary, c["name"] as? String ?? "")
        }
        let card = try XCTUnwrap(f["card"] as? [String: Any])
        for case let split as [String: Any] in try XCTUnwrap(card["splits"] as? [Any]) {
            let opened = try openCard(cardJson: json(split["envelope"]!), keys: keys(f))
            XCTAssertEqual(opened.status, .opened, split["name"] as? String ?? "")
            let shown = NSMutableDictionary(dictionary: split["card"] as! [AnyHashable: Any])
            // The `pictures` projection is never laid over (the sealed pictures stay whole).
            var over = split["clear"] as! [AnyHashable: Any]
            over["pictures"] = nil
            shown.addEntries(from: over)
            XCTAssertEqual(try JSONSerialization.jsonObject(with: Data(opened.cardJson.utf8)) as? NSDictionary, shown)
        }
    }
    #endif
}
