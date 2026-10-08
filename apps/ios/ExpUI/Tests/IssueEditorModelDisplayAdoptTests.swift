import Foundation
import XCTest
import ExpUI

// EXP-1238: a display-only row reloads IN PLACE. The streamed transcript
// grows one narration by fragments, and every fragment used to replace the
// row's model — new block ids, a new UITextView, a fresh measurement. The row
// now adopts the cached prototype's blocks under the ids it already renders,
// so the host's ForEach keeps the text view and re-applies its content.
@MainActor
final class IssueEditorModelDisplayAdoptTests: XCTestCase {
    private func prototype(_ markdown: String) -> IssueEditorModel {
        let model = IssueEditorModel()
        model.load(markdown: markdown, baseURL: nil)
        return model
    }

    func testGrowingTextKeepsTheBlockIdAndBumpsItsRevision() {
        let row = IssueEditorModel()
        row.adoptDisplayBlocks(from: prototype("Merge mechanics"))
        XCTAssertEqual(row.blocks.count, 1)
        let id = row.blocks[0].id
        let revision = row.revision(for: id)

        row.adoptDisplayBlocks(from: prototype("Merge mechanics worth knowing."))

        XCTAssertEqual(row.blocks.count, 1)
        XCTAssertEqual(row.blocks[0].id, id, "the row's text view keeps its identity")
        XCTAssertGreaterThan(row.revision(for: id), revision, "…and re-applies the new content")
        if case let .text(_, content) = row.blocks[0] {
            XCTAssertEqual(content.string, "Merge mechanics worth knowing.")
        } else {
            XCTFail("expected a text block")
        }
    }

    func testAnImageArrivingAppendsFreshBlocksAfterTheKeptOnes() {
        let row = IssueEditorModel()
        row.adoptDisplayBlocks(from: prototype("Before"))
        let textId = row.blocks[0].id

        row.adoptDisplayBlocks(from: prototype("Before\n\n![shot](/api/attachments/abc)\n\nAfter"))

        // text · image · text — the leading text keeps its id, the rest is new.
        XCTAssertEqual(row.blocks.count, 3)
        XCTAssertEqual(row.blocks[0].id, textId)
        guard case .image = row.blocks[1] else { return XCTFail("expected an image block") }
        XCTAssertNotEqual(row.blocks[1].id, textId)
        XCTAssertNotEqual(row.blocks[2].id, textId)
    }

    func testAdoptingIdOnlyAcrossTheSameKind() {
        let a = UUID(), b = UUID()
        let text = ContentBlock.text(id: a, attributedContent: NSAttributedString(string: "x"))
        let image = ContentBlock.image(id: b, url: "/api/attachments/1", alt: "")
        XCTAssertEqual(
            ContentBlock.text(id: UUID(), attributedContent: NSAttributedString(string: "y"))
                .adoptingId(of: text)?.id,
            a
        )
        XCTAssertNil(image.adoptingId(of: text), "an image never takes a text block's id")
        XCTAssertEqual(
            ContentBlock.attachmentLink(id: UUID(), url: "/api/attachments/2", label: "clip.mp4")
                .adoptingId(of: .attachmentLink(id: b, url: "/api/attachments/1", label: "old"))?.id,
            b
        )
    }

    func testPrototypeStaysUntouched() {
        let proto = prototype("Shared text")
        let protoId = proto.blocks[0].id
        let row = IssueEditorModel()
        let rowId = row.blocks[0].id
        row.adoptDisplayBlocks(from: proto)
        XCTAssertEqual(row.blocks[0].id, rowId, "the row keeps ITS id, not the prototype's")
        XCTAssertEqual(proto.blocks[0].id, protoId, "a cached prototype is never rewritten")
    }
}
