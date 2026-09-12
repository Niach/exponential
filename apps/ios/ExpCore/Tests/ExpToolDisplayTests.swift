import Foundation
import XCTest
@testable import ExpCore

// EXP-846: an Exponential MCP call renders as a caption ("Creating issue") plus
// the subject and a result preview, off the contract's `expToolDisplay` tables.
// The resolution rule is the load-bearing part: it must recognise our tools
// through EVERY namespace an adapter puts in front of them, and never claim
// another server's identically-named tool (desktop `mapper::exp_tool_row` and
// its test are the twin).
final class ExpToolDisplayTests: XCTestCase {

    func testResolvesOurToolsThroughEveryNamespace() {
        XCTAssertEqual(
            ExpToolDisplay.resolve(toolName: "mcp__exponential__exponential_issues_create")?.row,
            "issues_create"
        )
        XCTAssertEqual(ExpToolDisplay.resolve(toolName: "exponential_pr_open")?.row, "pr_open")
        XCTAssertEqual(
            ExpToolDisplay.resolve(toolName: "exponential.exponential_issues_list")?.row,
            "issues_list"
        )
        // Whitespace off the wire is trimmed, like every other published field.
        XCTAssertEqual(
            ExpToolDisplay.resolve(toolName: "  exponential_boards_create  ")?.row,
            "boards_create"
        )
    }

    func testRejectsEverythingElse() {
        // Another MCP server's tool of the same name: no `exponential_` prefix.
        XCTAssertNil(ExpToolDisplay.resolve(toolName: "mcp__linear__issues_create"))
        XCTAssertNil(ExpToolDisplay.resolve(toolName: "issues_create"))
        XCTAssertNil(ExpToolDisplay.resolve(toolName: "Bash"))
        XCTAssertNil(ExpToolDisplay.resolve(toolName: ""))
        // A plausible but non-contract row stays a plain tool row.
        XCTAssertNil(ExpToolDisplay.resolve(toolName: "exponential_issues_invented"))
    }

    func testCaptionsAndSubjectComeFromTheContract() {
        let create = ExpToolDisplay.resolve(toolName: "exponential_issues_create")
        XCTAssertEqual(create?.progressive, "Creating issue")
        XCTAssertEqual(create?.done, "Created issue")
        XCTAssertEqual(create?.caption(settled: false), "Creating issue")
        XCTAssertEqual(create?.caption(settled: true), "Created issue")
        XCTAssertEqual(create?.subjectKey, "title")
        XCTAssertEqual(create?.result, .issue)

        // A list tool names no subject and previews a count.
        let list = ExpToolDisplay.resolve(toolName: "exponential_issues_list")
        XCTAssertEqual(list?.subjectKey, "")
        XCTAssertEqual(list?.result, .list)

        XCTAssertEqual(ExpToolDisplay.resolve(toolName: "exponential_pr_open")?.result, .pr)
        XCTAssertEqual(
            ExpToolDisplay.resolve(toolName: "exponential_teams_update")?.result,
            ExpToolResultKind.none
        )
    }

    // Every contract row must resolve off its own bare name, or a tool the
    // server grew would render as a raw `exponential_*` string on this client.
    func testEveryContractRowResolves() {
        for row in DomainContract.expToolNames {
            let display = ExpToolDisplay.resolve(
                toolName: DomainContract.expToolPrefix + row
            )
            XCTAssertEqual(display?.row, row, "\(row) did not resolve")
            XCTAssertFalse(display?.progressive.isEmpty ?? true, "\(row) has no caption")
            XCTAssertFalse(display?.done.isEmpty ?? true, "\(row) has no done caption")
        }
    }

    // An unknown kind (a newer contract, an older client) is a caption-only
    // row, never a crash.
    func testUnknownResultKindFallsBackToNone() {
        XCTAssertEqual(ExpToolResultKind(wire: "galaxy"), ExpToolResultKind.none)
        XCTAssertEqual(ExpToolResultKind(wire: "issue"), ExpToolResultKind.issue)
        for kind in DomainContract.expToolResultKinds {
            XCTAssertEqual(ExpToolResultKind(wire: kind).rawValue, kind)
        }
    }
}
