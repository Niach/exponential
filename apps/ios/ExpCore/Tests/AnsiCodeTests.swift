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
}
