import Foundation
import XCTest
@testable import ExpCore

// #FEED-219: every composer's synced draft, locked ×4 against
// `packages/domain-contract/fixtures/composer-draft.json` — every case's
// WHOLE result. Read through `#filePath`: the unit-test bundle carries no
// repo resources.
final class ComposerDraftTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/composer-draft.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testDebounce() throws {
        XCTAssertEqual(try fixture()["debounceMs"] as? Int, ComposerDraft.debounceMs)
    }

    func testKeys() throws {
        let cases = try XCTUnwrap(try fixture()["keys"] as? [[String: Any]])
        XCTAssertEqual(cases.count, ComposerDraft.Kind.allCases.count)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let raw = try XCTUnwrap(entry["target"] as? [String: String], name)
            let kind = try XCTUnwrap(raw["kind"].flatMap(ComposerDraft.Kind.init(rawValue:)), name)
            let target = ComposerDraft.Target(kind: kind, id: try XCTUnwrap(raw["id"], name))
            let key = try XCTUnwrap(entry["key"] as? String, name)
            XCTAssertEqual(ComposerDraft.key(target), key, name)
            XCTAssertEqual(ComposerDraft.parse(key), target, name)
        }
    }

    func testUnparsable() throws {
        let keys = try XCTUnwrap(try fixture()["unparsable"] as? [String])
        XCTAssertFalse(keys.isEmpty)
        for key in keys {
            XCTAssertNil(ComposerDraft.parse(key), key)
        }
    }

    func testWrites() throws {
        let cases = try XCTUnwrap(try fixture()["writes"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let expected = try XCTUnwrap(entry["expected"] as? [String: String], name)
            let want: ComposerDraft.Write = expected["op"] == "upsert"
                ? .upsert(try XCTUnwrap(expected["text"], name))
                : .delete
            XCTAssertEqual(ComposerDraft.write(try XCTUnwrap(entry["text"] as? String, name)), want, name)
        }
    }

    func testFields() throws {
        let cases = try XCTUnwrap(try fixture()["fields"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            XCTAssertEqual(
                ComposerDraft.field(ComposerDraft.FieldInput(
                    field: try XCTUnwrap(input["field"] as? String, name),
                    edited: try XCTUnwrap(input["edited"] as? Bool, name),
                    local: input["local"] as? String,
                    synced: input["synced"] as? String
                )),
                try XCTUnwrap(entry["expected"] as? String, name),
                name
            )
        }
    }
}
