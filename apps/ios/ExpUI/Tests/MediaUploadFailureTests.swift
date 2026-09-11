import Foundation
import XCTest
import ExpCore
import ExpUI

// EXP-824: a queued video rides the SAME draft lifecycle as an inline image —
// a `draft://` placeholder behind an `attachmentLink` block, uploading /
// failed states, retry, and the real URL swapped in on success. And a failed
// upload must keep the save gate closed: `hasUncommittedDrafts` sees the
// link form too, so a `draft://` link never leaks into a saved body.
@MainActor
final class MediaUploadFailureTests: XCTestCase {
    private final class FailOnce: @unchecked Sendable {
        private let lock = NSLock()
        private var failed = false
        func shouldFail() -> Bool {
            lock.lock()
            defer { lock.unlock() }
            if failed { return false }
            failed = true
            return true
        }
    }

    private func clip() -> PendingImage {
        PendingImage(
            data: Data([0x1, 0x2]), filename: "clip.mp4", contentType: "video/mp4",
            width: 1280, height: 720, durationMs: 7250, poster: Data([0x9])
        )
    }

    private func modelWithDraftClip() -> (IssueEditorModel, UUID, String) {
        let model = IssueEditorModel()
        model.load(markdown: "hello", baseURL: nil)
        model.appendMedia(clip())
        let block = model.blocks.compactMap { block -> (UUID, String)? in
            if case .attachmentLink(let id, let url, _) = block { return (id, url) }
            return nil
        }.first!
        return (model, block.0, block.1)
    }

    func testAppendMediaInsertsADraftLinkBlock() {
        let (model, _, url) = modelWithDraftClip()
        XCTAssertTrue(MarkdownImageUtils.isDraft(url))
        XCTAssertEqual(model.currentMarkdown(), "hello\n\n[clip.mp4](\(url))")
        XCTAssertTrue(model.hasUncommittedDrafts)
        XCTAssertEqual(model.pendingImages[url]?.durationMs, 7250)
        XCTAssertEqual(model.pendingImages[url]?.mediaUploadParts?.poster, Data([0x9]))
    }

    func testInsertMediaSplitsAtTheCaret() {
        let model = IssueEditorModel()
        model.load(markdown: "before after", baseURL: nil)
        let textId = model.blocks.compactMap { block -> UUID? in
            if case .text(let id, _) = block { return id }
            return nil
        }.first!
        model.setFocused(textId)
        model.updateSelection(blockId: textId, range: NSRange(location: 6, length: 0))
        model.insertMedia(clip())
        let url = model.blocks.compactMap(\.draftableURL).first ?? ""
        XCTAssertEqual(model.currentMarkdown(), "before\n\n[clip.mp4](\(url))\n\nafter")
    }

    func testFailureKeepsTheDraftAndBlocksTheSave() async {
        let (model, blockId, url) = modelWithDraftClip()
        let saved = await model.commitPendingImages { _ in
            throw AttachmentsError.httpError(412, "storage full")
        }
        XCTAssertFalse(saved)
        XCTAssertEqual(model.uploadState(for: blockId), .failed(.storageFull))
        XCTAssertTrue(model.hasUncommittedDrafts)
        XCTAssertTrue(model.currentMarkdown().contains(url))
        XCTAssertNotNil(model.pendingImages[url])
    }

    func testSuccessSwapsTheLinkToTheRealAttachmentUrl() async {
        let (model, blockId, url) = modelWithDraftClip()
        let saved = await model.commitPendingImages { image in
            XCTAssertEqual(image.contentType, "video/mp4")
            XCTAssertEqual(image.mediaUploadParts?.width, 1280)
            return "/api/attachments/real"
        }
        XCTAssertTrue(saved)
        XCTAssertEqual(model.uploadState(for: blockId), .idle)
        XCTAssertFalse(model.hasUncommittedDrafts)
        XCTAssertNil(model.pendingImages[url])
        XCTAssertEqual(model.currentMarkdown(), "hello\n\n[clip.mp4](/api/attachments/real)")
    }

    func testRetryAfterFailureSwapsTheUrl() async {
        let (model, blockId, _) = modelWithDraftClip()
        let failOnce = FailOnce()
        _ = await model.commitPendingImages { _ in
            if failOnce.shouldFail() { throw AttachmentsError.httpError(500, "boom") }
            return "/api/attachments/real"
        }
        XCTAssertEqual(model.uploadState(for: blockId), .failed(.other))
        await model.retryImage(blockId: blockId)
        XCTAssertEqual(model.uploadState(for: blockId), .idle)
        XCTAssertEqual(model.currentMarkdown(), "hello\n\n[clip.mp4](/api/attachments/real)")
    }

    func testDeleteMediaBlockMergesItsNeighbours() {
        let (model, blockId, url) = modelWithDraftClip()
        model.deleteImageBlock(id: blockId)
        XCTAssertEqual(model.currentMarkdown(), "hello")
        XCTAssertNil(model.pendingImages[url])
    }

    func testBackspaceAfterMediaBlockDeletesIt() {
        let (model, _, url) = modelWithDraftClip()
        let afterId = model.blocks.last!.id
        model.deleteImage(beforeTextBlock: afterId)
        XCTAssertEqual(model.currentMarkdown(), "hello")
        XCTAssertNil(model.pendingImages[url])
    }

    func testDanglingDraftLinkIsDroppedBeforeCommit() async {
        let (model, _, url) = modelWithDraftClip()
        model.pendingImages[url] = nil
        let saved = await model.commitPendingImages { _ in "/api/attachments/never" }
        XCTAssertTrue(saved)
        XCTAssertEqual(model.currentMarkdown(), "hello")
    }

    func testAttachmentsDidChangeBumpsTheRevision() {
        let model = IssueEditorModel()
        let before = model.attachmentInfoRevision
        model.attachmentsDidChange()
        XCTAssertNotEqual(model.attachmentInfoRevision, before)
    }
}
