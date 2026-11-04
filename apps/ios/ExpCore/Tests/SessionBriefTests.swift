import Foundation
import XCTest
@testable import ExpCore

// EXP-1686: `agentForwardText`, locked ×4 against
// `packages/domain-contract/fixtures/session-brief.json` `forwardTextCases`.
final class SessionBriefTests: XCTestCase {
    func testReplaysEveryForwardTextCase() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/session-brief.json")
        let json = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
        let cases = try XCTUnwrap(json["forwardTextCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            XCTAssertEqual(
                SessionBrief.agentForwardText(
                    try XCTUnwrap(testCase["text"] as? String),
                    identifiers: try XCTUnwrap(testCase["identifiers"] as? [String])
                ),
                testCase["expected"] as? String,
                testCase["name"] as? String ?? ""
            )
        }
    }

    /// A bare forward carries its refs: the sealer seals a non-empty text.
    func testBareForwardCarriesTheRefs() {
        let input = SessionBrief.agentForwardInput(
            agentId: "a1", text: nil, attachmentIds: [], issueIds: ["i1", "i2"], identifiers: ["EXP-1", "EXP-2"]
        )
        XCTAssertEqual(
            input,
            .agent(agentId: "a1", text: "#EXP-1 #EXP-2", attachmentIds: nil, issueIds: ["i1", "i2"])
        )
    }

    /// Nothing to say and nothing forwarded stays off the wire, never `""`.
    func testEmptyTextIsOmitted() {
        for text in [nil, "", "  \n"] as [String?] {
            XCTAssertEqual(
                SessionBrief.agentForwardInput(
                    agentId: "a1", text: text, attachmentIds: ["f1"], issueIds: [], identifiers: []
                ),
                .agent(agentId: "a1", text: nil, attachmentIds: ["f1"], issueIds: nil)
            )
        }
    }

    func testTextKeepsItsWordsBehindTheRefs() {
        XCTAssertEqual(
            SessionBrief.agentForwardInput(
                agentId: "a1", text: "Please look.\n", attachmentIds: [], issueIds: ["i1"], identifiers: ["EXP-1"]
            ).text,
            "#EXP-1\n\nPlease look."
        )
    }
}
