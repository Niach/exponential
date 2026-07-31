import XCTest
@testable import ExpCore

// EXP-390: the github-connected deep link's error leg — the slugs are the
// server's callback error codes, and every known one must map to copy more
// specific than the generic fallback (mirror of the desktop's
// error_copy_covers_every_server_slug).
final class GithubConnectTests: XCTestCase {
    func testKnownSlugsBeatTheFallback() {
        let fallback = GithubConnect.errorMessage(for: "definitely-unknown")
        for slug in ["session", "exchange", "none", "notowner", "orgperm", "forbidden"] {
            XCTAssertNotEqual(GithubConnect.errorMessage(for: slug), fallback, slug)
        }
    }

    func testErrorSlugParsesTheErrorQuery() {
        let url = URL(string: "exponential://github-connected?error=session")!
        XCTAssertEqual(GithubConnect.errorSlug(from: url), "session")
    }

    func testSuccessFormsHaveNoSlug() {
        for raw in [
            "exponential://github-connected",
            "exponential://github-connected/",
            "exponential://github-connected?",
            "exponential://github-connected?error=",
            "exponential://github-connected#frag",
        ] {
            XCTAssertNil(GithubConnect.errorSlug(from: URL(string: raw)!), raw)
        }
    }
}
