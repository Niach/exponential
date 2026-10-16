import Foundation
import XCTest
@testable import ExpCore

// EXP-1324: the CodeBlock's `ansi` language, locked against the `ansi` cases
// of `packages/exponential-ui/fixtures/code-tokens.json` (the eUI built-in
// tokenizer's fixture) — every line's whole token list.
final class AnsiCodeTests: XCTestCase {
    private func cases() throws -> [[String: Any]] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/exponential-ui/fixtures/code-tokens.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        let root = try XCTUnwrap(json as? [String: Any])
        let all = try XCTUnwrap(root["cases"] as? [[String: Any]])
        return all.filter { $0["language"] as? String == "ansi" }
    }

    func testAnsiCases() throws {
        let all = try cases()
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let name = "\(c["name"] ?? "")"
            let code = try XCTUnwrap(c["code"] as? String)
            let expected = try XCTUnwrap(c["expected"] as? [Any]).map { line in
                ((line as? [Any]) ?? []).compactMap { $0 as? [String: Any] }.map {
                    CodeToken(kind: $0["kind"] as? String ?? "", text: $0["text"] as? String ?? "")
                }
            }
            let got = AnsiCode.tokenize(code)
            XCTAssertEqual(got, expected, name)
            // The shown + copied text = every line's tokens, escape-free.
            XCTAssertEqual(
                AnsiCode.text(code),
                got.map { $0.map(\.text).joined() }.joined(separator: "\n"),
                name
            )
        }
    }

    /// The prefix match is on UTF-16 units like TS `startsWith`: an
    /// emoji-presentation check/cross (`U+2714 U+FE0F`, `U+2717 U+FE0F`)
    /// still starts with the bare glyph (Swift's grapheme-aware `hasPrefix`
    /// would say it does not).
    func testEmojiPresentationPrefixes() {
        XCTAssertEqual(
            AnsiCode.tokenize("\u{2714}\u{FE0F} done\n  \u{2717}\u{FE0F} failed\nplain \u{2714}\u{FE0F}"),
            [
                [CodeToken(kind: "success", text: "\u{2714}\u{FE0F} done")],
                [CodeToken(kind: "plain", text: "  "), CodeToken(kind: "error", text: "\u{2717}\u{FE0F} failed")],
                [CodeToken(kind: "plain", text: "plain \u{2714}\u{FE0F}")],
            ]
        )
    }
}
