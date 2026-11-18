import Foundation
import XCTest
@testable import ExpCore

// #EXP-1949: replays packages/domain-contract/fixtures/image-actions.json
// whole (TS `image-actions.test.ts`, Android `ImageActionsTest`): `menuCases`,
// `pressCases`, `encodingCases`, `shareNameCases`, `sourceCases`, and locks
// the copy, the icons and the test ids.
final class ImageActionsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/image-actions.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testConstants() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["copy"] as? [String: String], [
            "copyImage": ImageActions.Copy.copyImage, "share": ImageActions.Copy.share,
            "copied": ImageActions.Copy.copied, "copyFailed": ImageActions.Copy.copyFailed,
            "shareFailed": ImageActions.Copy.shareFailed, "linkHost": ImageActions.Copy.linkHost,
        ])
        XCTAssertEqual(fixture["icons"] as? [String: String], ["copyImage": ImageActions.Icons.copyImage, "share": ImageActions.Icons.share])
        XCTAssertEqual(fixture["testIds"] as? [String: String], ["copyImage": ImageActions.TestIds.copyImage, "share": ImageActions.TestIds.share])
        let inputs = try XCTUnwrap(fixture["inputs"] as? [String: [String]])
        XCTAssertEqual(inputs["ios"], ["touch"])
    }

    func testMenuCases() throws {
        let cases = try XCTUnwrap(try fixture()["menuCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let input = try XCTUnwrap(ImageActions.Input(rawValue: try XCTUnwrap(c["input"] as? String)))
            let canShare = try XCTUnwrap(c["canShareFile"] as? Bool)
            XCTAssertEqual(ImageActions.imageMenu(input, canShareFile: canShare).map(\.rawValue), c["expected"] as? [String], "\(input) \(canShare)")
        }
    }

    func testPressCases() throws {
        for c in try XCTUnwrap(try fixture()["pressCases"] as? [[String: Any]]) {
            let actual = ImageActions.pressMenu(
                onImage: try XCTUnwrap(c["onImage"] as? Bool),
                imageReady: try XCTUnwrap(c["imageReady"] as? Bool),
                ownerMenu: c["ownerMenu"] as? String
            )
            XCTAssertEqual(actual.rawValue, c["expected"] as? String, "\(c)")
        }
    }

    func testEncodingCases() throws {
        for c in try XCTUnwrap(try fixture()["encodingCases"] as? [[String: Any]]) {
            let hex = Array(try XCTUnwrap(c["head"] as? String))
            let bytes = stride(from: 0, to: hex.count, by: 2).map { UInt8(String(hex[$0..<$0 + 2]), radix: 16)! }
            XCTAssertEqual(ImageActions.copyEncoding(Data(bytes)).rawValue, c["expected"] as? String, "\(c)")
        }
    }

    func testShareNameCases() throws {
        for c in try XCTUnwrap(try fixture()["shareNameCases"] as? [[String: Any]]) {
            XCTAssertEqual(ImageActions.shareFileName(c["name"] as? String), c["expected"] as? String, "\(c)")
        }
    }

    func testSourceCases() throws {
        let fixture = try fixture()
        let origin = try XCTUnwrap((fixture["sourcePolicy"] as? [String: Any])?["origin"] as? String)
        let cases = try XCTUnwrap(fixture["sourceCases"] as? [[String: Any]])
        XCTAssertGreaterThan(cases.count, 40)
        for c in cases {
            let e = try XCTUnwrap(c["expected"] as? [String: String])
            let expected: ImageActions.Source = switch e["kind"] {
            case "load": .load
            case "link": .link(href: try XCTUnwrap(e["href"]), text: try XCTUnwrap(e["text"]))
            default: .text(try XCTUnwrap(e["text"]))
            }
            let src = try XCTUnwrap(c["src"] as? String), alt = try XCTUnwrap(c["alt"] as? String)
            XCTAssertEqual(ImageActions.imageSource(src, alt: alt, origin: origin), expected, "\(c)")
        }
    }

    func testAttachmentIdAndOriginalSrc() {
        let id = "0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90"
        XCTAssertEqual(ImageActions.attachmentId("/api/attachments/\(id)?w=480", origin: "https://app.exponential.at"), id)
        XCTAssertEqual(ImageActions.attachmentId("https://app.exponential.at/api/attachments/\(id)", origin: "https://app.exponential.at"), id)
        XCTAssertNil(ImageActions.attachmentId("https://example.com/api/attachments/\(id)", origin: "https://app.exponential.at"))
        XCTAssertEqual(ImageActions.originalSrc("/api/attachments/\(id)?w=480"), "/api/attachments/\(id)")
        XCTAssertEqual(ImageActions.originalSrc("/api/attachments/\(id)?token=t&w=480#x"), "/api/attachments/\(id)?token=t#x")
        XCTAssertEqual(ImageActions.originalSrc("data:image/png;base64,AA?w=1"), "data:image/png;base64,AA?w=1")
    }
}
