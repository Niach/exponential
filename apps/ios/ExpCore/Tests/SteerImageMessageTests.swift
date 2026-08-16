import Foundation
import XCTest
@testable import ExpCore

// EXP-511: the steer image message shape is a cross-client contract — the host
// rewrites each embed to a local file path and reverse-rewrites the echo, so
// these fixtures are byte-identical to the web
// (apps/web/src/lib/steer-image-message.test.ts) and Android twins.
final class SteerImageMessageTests: XCTestCase {
    private let idA = "11111111-1111-4111-8111-111111111111"
    private let idB = "22222222-2222-4222-8222-222222222222"

    func testTextAndImagesJoinWithABlankLine() {
        XCTAssertEqual(
            SteerImageMessage.build(text: "fix the header", attachmentIds: [idA, idB]),
            """
            fix the header

            ![image](/api/attachments/11111111-1111-4111-8111-111111111111)
            ![image](/api/attachments/22222222-2222-4222-8222-222222222222)
            """
        )
    }

    func testBlankTextYieldsEmbedsOnly() {
        XCTAssertEqual(
            SteerImageMessage.build(text: "  \n ", attachmentIds: [idA]),
            "![image](/api/attachments/11111111-1111-4111-8111-111111111111)"
        )
    }

    func testNoImagesTrimsTheText() {
        XCTAssertEqual(SteerImageMessage.build(text: "  hello  ", attachmentIds: []), "hello")
    }

    func testEmptyTextAndNoImagesIsEmpty() {
        XCTAssertEqual(SteerImageMessage.build(text: "", attachmentIds: []), "")
    }

    func testMaxImagesMatchesTheOtherClients() {
        XCTAssertEqual(SteerImageMessage.maxImages, 4)
    }
}
