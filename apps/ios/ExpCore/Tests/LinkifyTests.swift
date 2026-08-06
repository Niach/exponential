import XCTest

@testable import ExpCore

// EXP-430: narration URLs become tappable links — the remote `/login` flow
// publishes the claude sign-in URL as narration, and it must tokenize intact.
// Mirrors the web `lib/linkify.test.ts` and Android `LinkifyTest`.
final class LinkifyTests: XCTestCase {

    private let signInUrl =
        "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e"
        + "&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback"
        + "&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainference&code_challenge=j7BY1qKMJ1Y2LC5xNqD5"
        + "VUJayK_UZbPl_FCJLsmPZzk&code_challenge_method=S256&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE"

    func testPlainTextIsOneSegment() {
        XCTAssertEqual(linkSegments("Session started"), [LinkSegment(text: "Session started")])
        XCTAssertEqual(linkSegments(""), [LinkSegment(text: "")])
    }

    func testSignInUrlSurvivesIntact() {
        let text = "Claude sign-in: open this link in your browser:\n\n\(signInUrl)"
        let segments = linkSegments(text)
        XCTAssertEqual(segments.count, 2)
        XCTAssertEqual(segments[1], LinkSegment(text: signInUrl, href: signInUrl))
        XCTAssertEqual(segments.map(\.text).joined(), text)
    }

    func testUrlInProseKeepsSurroundingText() {
        XCTAssertEqual(
            linkSegments("Opened https://github.com/x/y/pull/12 for review"),
            [
                LinkSegment(text: "Opened "),
                LinkSegment(text: "https://github.com/x/y/pull/12", href: "https://github.com/x/y/pull/12"),
                LinkSegment(text: " for review"),
            ])
    }

    func testTrailingProsePunctuationStaysOutOfTheLink() {
        XCTAssertEqual(
            linkSegments("(see https://x.dev/docs)."),
            [
                LinkSegment(text: "(see "),
                LinkSegment(text: "https://x.dev/docs", href: "https://x.dev/docs"),
                LinkSegment(text: ")."),
            ])
    }

    func testBalancedParensStayInTheLink() {
        let url = "https://en.wikipedia.org/wiki/Bracket_(disambiguation)"
        XCTAssertEqual(linkSegments(url), [LinkSegment(text: url, href: url)])
    }

    func testMultipleUrlsAllLink() {
        let text = "a https://one.test b http://two.test c"
        let segments = linkSegments(text)
        XCTAssertEqual(segments.filter { $0.href != nil }.count, 2)
        XCTAssertEqual(segments.map(\.text).joined(), text)
    }

    func testLinkifiedNarrationCarriesLinkRuns() {
        let attributed = linkifiedNarration("open https://x.dev now")
        let links = attributed.runs.compactMap(\.link)
        XCTAssertEqual(links, [URL(string: "https://x.dev")!])
    }
}
