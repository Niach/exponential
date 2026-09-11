import XCTest
@testable import ExpCore

/// EXP-824 — the bearer rides an AVURLAsset request ONLY on the account's own
/// origin. Same rule as the image loader; a foreign host never sees the token.
final class AttachmentMediaAuthTests: XCTestCase {
    private let base = URL(string: "https://app.example.com")!

    func testRelativeAttachmentUrlResolvesWithBearerAndVersion() throws {
        let request = try XCTUnwrap(AttachmentMediaAuth.request(
            for: "/api/attachments/a1", instanceBaseURL: base, token: "tok"
        ))
        XCTAssertEqual(request.url.absoluteString, "https://app.example.com/api/attachments/a1")
        XCTAssertEqual(request.headers["Authorization"], "Bearer tok")
        XCTAssertEqual(request.headers["x-client-version"], AppConstants.clientVersionHeaderValue)
    }

    func testSameOriginAbsoluteUrlCarriesTheBearer() {
        let headers = AttachmentMediaAuth.headers(
            for: URL(string: "https://app.example.com:443/api/attachments/a1?poster=1")!,
            instanceBaseURL: base,
            token: "tok"
        )
        XCTAssertEqual(headers["Authorization"], "Bearer tok")
    }

    func testForeignHostGetsNoHeadersEvenWithAnApiPath() {
        let headers = AttachmentMediaAuth.headers(
            for: URL(string: "https://evil.example.com/api/attachments/a1")!,
            instanceBaseURL: base,
            token: "tok"
        )
        XCTAssertTrue(headers.isEmpty)
    }

    func testSchemeOrPortMismatchGetsNoHeaders() {
        XCTAssertTrue(AttachmentMediaAuth.headers(
            for: URL(string: "http://app.example.com/api/attachments/a1")!,
            instanceBaseURL: base, token: "tok"
        ).isEmpty)
        XCTAssertTrue(AttachmentMediaAuth.headers(
            for: URL(string: "https://app.example.com:8443/api/attachments/a1")!,
            instanceBaseURL: base, token: "tok"
        ).isEmpty)
    }

    func testMissingTokenOrInstanceGetsNoHeaders() {
        let url = URL(string: "https://app.example.com/api/attachments/a1")!
        XCTAssertTrue(AttachmentMediaAuth.headers(for: url, instanceBaseURL: base, token: nil).isEmpty)
        XCTAssertTrue(AttachmentMediaAuth.headers(for: url, instanceBaseURL: base, token: "").isEmpty)
        XCTAssertTrue(AttachmentMediaAuth.headers(for: url, instanceBaseURL: nil, token: "tok").isEmpty)
    }
}
