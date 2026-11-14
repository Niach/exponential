import Foundation
import XCTest
@testable import ExpCore

// #EXP-1841: THE composer strip and its `[Image #N]` placeholders, locked ×4
// against `packages/domain-contract/fixtures/composer-attachments.json`:
// ALL four case lists (insert, renumber, stage, drop), each case's WHOLE
// expected value. Read through `#filePath`: the unit-test bundle carries no
// repo resources.
final class ComposerAttachmentsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/composer-attachments.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ list: String) throws -> [[String: Any]] {
        let cases = try XCTUnwrap(try fixture()[list] as? [[String: Any]], list)
        XCTAssertFalse(cases.isEmpty, list)
        return cases
    }

    private func kinds(_ raw: Any?, _ name: String) throws -> [ComposerAttachments.Kind] {
        try XCTUnwrap(raw as? [String], name).map { try XCTUnwrap(ComposerAttachments.Kind(rawValue: $0), name) }
    }

    private func picks(_ raw: Any?, _ name: String) throws -> [ComposerAttachments.Pick] {
        try XCTUnwrap(raw as? [String], name).map { try XCTUnwrap(ComposerAttachments.Pick(rawValue: $0), name) }
    }

    func testInsertReplays() throws {
        for entry in try cases("insert") {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let expected = try XCTUnwrap(entry["expected"] as? [String: Any], name)
            XCTAssertEqual(Set(expected.keys), ["text", "caret"], name)
            let result = ComposerAttachments.insertImageMarker(
                text: try XCTUnwrap(input["text"] as? String, name),
                caret: try XCTUnwrap(input["caret"] as? Int, name),
                index: try XCTUnwrap(input["index"] as? Int, name)
            )
            XCTAssertEqual(result.text, try XCTUnwrap(expected["text"] as? String, name), name)
            XCTAssertEqual(result.caret, try XCTUnwrap(expected["caret"] as? Int, name), name)
        }
    }

    func testRenumberReplays() throws {
        for entry in try cases("renumber") {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            XCTAssertEqual(
                ComposerAttachments.renumberImageMarkers(
                    try XCTUnwrap(input["text"] as? String, name),
                    removedIndex: try XCTUnwrap(input["removed"] as? Int, name)
                ),
                try XCTUnwrap(entry["expected"] as? String, name),
                name
            )
        }
    }

    func testStageReplays() throws {
        for entry in try cases("stage") {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let expected = try XCTUnwrap(entry["expected"] as? [String: Any], name)
            XCTAssertEqual(
                Set(expected.keys),
                ["strip", "taken", "text", "caret", "rejected", "overflow", "fileOverflow", "toasts"],
                name
            )
            let result = ComposerAttachments.stage(
                strip: try kinds(input["strip"], name),
                add: try picks(input["add"], name),
                text: try XCTUnwrap(input["text"] as? String, name),
                caret: try XCTUnwrap(input["caret"] as? Int, name)
            )
            XCTAssertEqual(
                result,
                ComposerAttachments.Staged(
                    strip: try kinds(expected["strip"], name),
                    taken: try XCTUnwrap(expected["taken"] as? [Bool], name),
                    text: try XCTUnwrap(expected["text"] as? String, name),
                    caret: try XCTUnwrap(expected["caret"] as? Int, name),
                    rejected: try XCTUnwrap(expected["rejected"] as? Int, name),
                    overflow: try XCTUnwrap(expected["overflow"] as? Int, name),
                    fileOverflow: try XCTUnwrap(expected["fileOverflow"] as? Int, name)
                ),
                name
            )
            XCTAssertEqual(result.toasts, try XCTUnwrap(expected["toasts"] as? [String], name), name)
        }
    }

    func testDropReplays() throws {
        for entry in try cases("drop") {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let expected = try XCTUnwrap(entry["expected"] as? [String: Any], name)
            XCTAssertEqual(Set(expected.keys), ["strip", "text", "caret"], name)
            let result = ComposerAttachments.drop(
                strip: try kinds(input["strip"], name),
                at: try XCTUnwrap(input["at"] as? Int, name),
                text: try XCTUnwrap(input["text"] as? String, name),
                caret: try XCTUnwrap(input["caret"] as? Int, name)
            )
            XCTAssertEqual(result.strip, try kinds(expected["strip"], name), name)
            XCTAssertEqual(result.text, try XCTUnwrap(expected["text"] as? String, name), name)
            XCTAssertEqual(result.caret, try XCTUnwrap(expected["caret"] as? Int, name), name)
        }
    }

    /// The forwarders stay the same edit (ONE implementation).
    func testSteerImageMessageForwards() {
        XCTAssertEqual(
            SteerImageMessage.insertImageMarker(text: "see", caret: 3, index: 1).text,
            ComposerAttachments.insertImageMarker(text: "see", caret: 3, index: 1).text
        )
        XCTAssertEqual(
            SteerImageMessage.renumberImageMarkers("[Image #1] x [Image #2]", removedIndex: 1),
            "x [Image #1]"
        )
    }
}
