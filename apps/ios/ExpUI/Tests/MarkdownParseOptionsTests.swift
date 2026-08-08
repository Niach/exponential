import Foundation
import UIKit
import XCTest
import ExpUI

// EXP-440: the agent steering feed renders agent prose as markdown, which needs
// two deviations from the interchange contract that only a DISPLAY-ONLY render
// may take. These lock both halves: the deviations do what the feed needs, and
// the default parse (every editable surface) still refuses them.
//
// The bare-URL half also inherits EXP-430's contract — the remote `/login` flow
// publishes the claude sign-in URL as narration and it must stay tappable and
// byte-intact. Ported from the hand tokenizer this replaced (ExpCore
// LinkifyTests), which cmark's GFM autolink extension now does instead.
final class MarkdownParseOptionsTests: XCTestCase {

    private let signInUrl =
        "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e"
        + "&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback"
        + "&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainference&code_challenge=j7BY1qKMJ1Y2LC5xNqD5"
        + "VUJayK_UZbPl_FCJLsmPZzk&code_challenge_method=S256&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE"

    /// The rendered text of `markdown` — the first non-empty text block, which
    /// is the whole document for the image-free sources here.
    private func text(of markdown: String, options: MarkdownParseOptions) -> NSAttributedString {
        let blocks = MarkdownConversion.markdownToBlocks(markdown, options: options)
        for block in blocks {
            if case let .text(_, content) = block, content.length > 0 { return content }
        }
        return NSAttributedString()
    }

    /// Every `.link` run as (href, displayed text).
    private func linkRuns(in attributed: NSAttributedString) -> [(href: String, text: String)] {
        var runs: [(href: String, text: String)] = []
        attributed.enumerateAttribute(
            .link,
            in: NSRange(location: 0, length: attributed.length),
            options: []
        ) { value, range, _ in
            guard let href = (value as? URL)?.absoluteString ?? (value as? String) else { return }
            runs.append((href, (attributed.string as NSString).substring(with: range)))
        }
        return runs
    }

    // MARK: - autolinkBareURLs

    func testBareURLLinksWithItsExactHref() {
        let runs = linkRuns(in: text(of: "See https://example.com/docs", options: [.autolinkBareURLs]))
        XCTAssertEqual(runs.count, 1)
        XCTAssertEqual(runs.first?.href, "https://example.com/docs")
        XCTAssertEqual(runs.first?.text, "https://example.com/docs")
    }

    func testSignInUrlSurvivesIntact() {
        let markdown = "Claude sign-in: open this link in your browser:\n\n\(signInUrl)"
        let runs = linkRuns(in: text(of: markdown, options: [.autolinkBareURLs]))
        XCTAssertEqual(runs.count, 1)
        XCTAssertEqual(runs.first?.href, signInUrl)
        XCTAssertEqual(runs.first?.text, signInUrl)
    }

    func testTrailingProsePunctuationStaysOutOfTheLink() {
        let runs = linkRuns(in: text(of: "see https://x.dev.", options: [.autolinkBareURLs]))
        XCTAssertEqual(runs.count, 1)
        XCTAssertEqual(runs.first?.href, "https://x.dev")
        XCTAssertEqual(runs.first?.text, "https://x.dev")
    }

    func testBalancedParensStayInTheLink() {
        let url = "https://x.dev/a(b)"
        let runs = linkRuns(in: text(of: url, options: [.autolinkBareURLs]))
        XCTAssertEqual(runs.count, 1)
        XCTAssertEqual(runs.first?.text, url)
    }

    /// The interchange contract: without the option a bare URL is PLAIN TEXT,
    /// so the load→save cycle can never rewrite it (and can never turn the
    /// email of an `@<email>` mention into a mailto link).
    func testBareURLsAndEmailsStayPlainByDefault() {
        XCTAssertTrue(linkRuns(in: text(of: "See https://example.com/docs", options: [])).isEmpty)
        XCTAssertTrue(linkRuns(in: text(of: "ping @sam@example.com", options: [])).isEmpty)
    }

    /// Explicit markdown links are the parser's baseline either way.
    func testExplicitLinkIsUnaffectedByTheOption() {
        let cases: [MarkdownParseOptions] = [[], [.autolinkBareURLs]]
        for options in cases {
            let runs = linkRuns(in: text(of: "a [docs](https://example.com/docs) b", options: options))
            XCTAssertEqual(runs.count, 1)
            XCTAssertEqual(runs.first?.href, "https://example.com/docs")
            XCTAssertEqual(runs.first?.text, "docs")
        }
    }

    // MARK: - hardLineBreaks

    func testHardLineBreaksKeepASingleNewline() {
        XCTAssertEqual(text(of: "line1\nline2", options: [.hardLineBreaks]).string, "line1\nline2")
    }

    func testSoftbreakFoldsToASpaceByDefault() {
        XCTAssertEqual(text(of: "line1\nline2", options: []).string, "line1 line2")
    }
}
