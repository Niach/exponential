import Foundation
import XCTest
@testable import ExpCore

// Compliance lock (EXP-216 / App Store 3.1.1): the images route answers the
// team storage cap with HTTP 412 whose body is the server's billing copy
// ("Upgrade to upload more."). Any surface rendering the error's description —
// the Share Extension does — must get neutral copy instead, while the raw body
// stays on the case so the editor's storage-full classification keeps matching.
final class IssueImagesErrorTests: XCTestCase {
    private let storageCapBody = """
        {"error":"Your plan allows up to 250 MB of attachment storage. Upgrade to upload more."}
        """

    func testStorageCapDescriptionIsNeutral() {
        let error = IssueImagesError.httpError(412, storageCapBody)
        XCTAssertEqual(error.errorDescription, storageFullNeutralMessage)
        XCTAssertEqual(error.localizedDescription, storageFullNeutralMessage)
        XCTAssertFalse(storageFullNeutralMessage.localizedCaseInsensitiveContains("upgrade"))
        XCTAssertFalse(storageFullNeutralMessage.localizedCaseInsensitiveContains("plan"))
    }

    func testStorageCapMessageIsNeutralThroughTrpcUserMessage() {
        // The Share Extension routes every submit failure through this
        // chokepoint; a non-tRPC error falls back to the neutral description.
        let error = IssueImagesError.httpError(412, storageCapBody)
        XCTAssertEqual(error.trpcUserMessage, storageFullNeutralMessage)
    }

    func testStorageCapKeepsRawBodyOnTheCase() {
        // ExpUI classifies on `.httpError(412, _)` — the body must survive.
        let error = IssueImagesError.httpError(412, storageCapBody)
        guard case let .httpError(code, body) = error else {
            return XCTFail("expected httpError")
        }
        XCTAssertEqual(code, 412)
        XCTAssertEqual(body, storageCapBody)
    }

    func testOtherStatusCodesKeepTheDiagnosticDescription() {
        let error = IssueImagesError.httpError(500, "boom")
        XCTAssertEqual(error.errorDescription, "Image upload failed: HTTP 500 boom")
    }
}
