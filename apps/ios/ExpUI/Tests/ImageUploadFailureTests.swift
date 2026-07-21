import Foundation
import XCTest
import ExpCore
import ExpUI

// EXP-216: a storage-cap upload failure (HTTP 412 from the images route) must
// surface as `.failed(.storageFull)` so the editor can explain the failure
// with neutral copy instead of a bare retry state; any other error stays
// `.failed(.other)`, and a successful retry clears back to `.idle`.
@MainActor
final class ImageUploadFailureTests: XCTestCase {
    /// Thread-safe fail-once switch, `@Sendable`-closure friendly.
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

    private func modelWithDraftImage() -> (IssueEditorModel, UUID) {
        let model = IssueEditorModel()
        model.load(markdown: "", baseURL: nil)
        model.insertImage(data: Data([0x1]), filename: "a.png", contentType: "image/png", width: 4, height: 3)
        let imageId = model.blocks.compactMap { block -> UUID? in
            if case .image(let id, _, _) = block { return id }
            return nil
        }.first
        return (model, imageId!)
    }

    func testStorageFullFailureIsClassified() async {
        let (model, imageId) = modelWithDraftImage()
        let saved = await model.commitPendingImages { _ in
            throw IssueImagesError.httpError(412, "Your plan allows up to 250 MB of attachment storage. Upgrade to upload more.")
        }
        XCTAssertFalse(saved)
        XCTAssertEqual(model.uploadState(for: imageId), .failed(.storageFull))
    }

    func testOtherFailureIsClassifiedAsOther() async {
        let (model, imageId) = modelWithDraftImage()
        let saved = await model.commitPendingImages { _ in
            throw IssueImagesError.httpError(500, "boom")
        }
        XCTAssertFalse(saved)
        XCTAssertEqual(model.uploadState(for: imageId), .failed(.other))
    }

    func testRetryAfterFailureClearsToIdle() async {
        let (model, imageId) = modelWithDraftImage()
        let failOnce = FailOnce()
        _ = await model.commitPendingImages { _ in
            if failOnce.shouldFail() {
                throw IssueImagesError.httpError(412, "storage full")
            }
            return "/api/attachments/fake"
        }
        XCTAssertEqual(model.uploadState(for: imageId), .failed(.storageFull))
        await model.retryImage(blockId: imageId)
        XCTAssertEqual(model.uploadState(for: imageId), .idle)
        // Retry re-ran the remembered uploader and swapped the block URL.
        if case .image(_, let url, _) = model.blocks.first(where: { $0.id == imageId }) {
            XCTAssertEqual(url, "/api/attachments/fake")
        } else {
            XCTFail("image block missing after retry")
        }
    }
}
