import Foundation
import XCTest
@testable import ExpCore

// EXP-1233: the pull request the Fix merge conflicts builtin picked, resolved
// off the synced rows (web `resolveFixConflictsPr`), and the card's branch
// line (web `branchLine`), ×4.
final class FixConflictsPrTests: XCTestCase {
    private func issue(
        _ id: String,
        identifier: String,
        prUrl: String? = "https://github.com/acme/app/pull/2117",
        prState: String? = "open",
        branch: String? = "exp/APP-14",
        base: String? = "master"
    ) -> IssueEntity {
        IssueEntity(
            id: id, boardId: "b", number: nil, identifier: identifier,
            title: "Title \(identifier)", description: nil, status: "in_review",
            priority: "none", assigneeId: nil, creatorId: nil, source: nil,
            dueDate: nil, sortOrder: nil, completedAt: nil, duplicateOfId: nil,
            prUrl: prUrl, prNumber: prUrl == nil ? nil : 2117, prState: prState,
            branch: branch, prBaseBranch: base, prMergedAt: nil,
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
    }

    func testBranchLine() {
        XCTAssertEqual(FixConflictsPr.branchLine(branch: "exp/APP-14", base: "master"), "exp/APP-14 → master")
        XCTAssertEqual(FixConflictsPr.branchLine(branch: "exp/APP-14", base: nil), "exp/APP-14")
        XCTAssertEqual(FixConflictsPr.branchLine(branch: "exp/APP-14", base: ""), "exp/APP-14")
        XCTAssertEqual(FixConflictsPr.branchLine(branch: nil, base: "master"), "")
        XCTAssertEqual(FixConflictsPr.branchLine(branch: "", base: "master"), "")
    }

    func testNothingPickedOrUnsyncedIsNil() {
        let rows = [issue("a", identifier: "APP-14")]
        XCTAssertNil(FixConflictsPr.resolve(prIssueId: nil, issues: rows))
        XCTAssertNil(FixConflictsPr.resolve(prIssueId: "", issues: rows))
        XCTAssertNil(FixConflictsPr.resolve(prIssueId: "zzz", issues: rows))
    }

    func testSingleIssuePr() {
        let pr = FixConflictsPr.resolve(
            prIssueId: "a",
            issues: [issue("a", identifier: "APP-14"), issue("x", identifier: "APP-2", prUrl: "other")]
        )
        XCTAssertEqual(pr?.issueId, "a")
        XCTAssertEqual(pr?.prNumber, 2117)
        XCTAssertEqual(pr?.branchLine, "exp/APP-14 → master")
        XCTAssertEqual(pr?.issues.map(\.identifier), ["APP-14"])
        XCTAssertEqual(pr?.issues.first?.title, "Title APP-14")
        XCTAssertEqual(pr?.issues.first?.status, "in_review")
    }

    // A batch PR shows EVERY open-PR issue sharing its URL, by identifier; a
    // closed row on the same URL does not count.
    func testBatchPrListsEveryLinkedOpenIssueSorted() {
        let rows = [
            issue("c", identifier: "APP-20"),
            issue("a", identifier: "APP-14"),
            issue("b", identifier: "APP-15"),
            issue("d", identifier: "APP-16", prState: "merged"),
        ]
        let pr = FixConflictsPr.resolve(prIssueId: "c", issues: rows)
        XCTAssertEqual(pr?.issueId, "c")
        XCTAssertEqual(pr?.issues.map(\.identifier), ["APP-14", "APP-15", "APP-20"])
    }

    func testNoBaseKeepsTheBranchAlone() {
        let pr = FixConflictsPr.resolve(prIssueId: "a", issues: [issue("a", identifier: "APP-14", base: nil)])
        XCTAssertEqual(pr?.branchLine, "exp/APP-14")
    }
}
