import Foundation
import XCTest
@testable import ExpCore

// EXP-897 — the blocked-start rule's three tests, the same three web
// (`stack-start.test.ts`), Android (`StackStartTest`) and the desktop
// (`blockers_of_*`) run.
final class StackStartTests: XCTestCase {
    private func issue(_ id: String, identifier: String, status: String = "backlog") -> IssueEntity {
        IssueEntity(
            id: id, boardId: "b1", number: 1, identifier: identifier, title: "T \(identifier)",
            description: nil, status: status, priority: "none", assigneeId: nil, creatorId: nil,
            source: nil, dueDate: nil, sortOrder: 1, completedAt: nil, duplicateOfId: nil,
            prUrl: nil, prNumber: nil, prState: nil, branch: nil, prMergedAt: nil,
            createdAt: "2026-09-16T09:00:00Z", updatedAt: "2026-09-16T09:00:00Z"
        )
    }

    private func relation(_ id: String, from: String, to: String, type: String) -> IssueRelationEntity {
        IssueRelationEntity(
            id: id, issueId: from, relatedIssueId: to, type: type, source: "user",
            teamId: "t1", boardId: "b1",
            createdAt: "2026-09-16T09:00:00Z", updatedAt: "2026-09-16T09:00:00Z"
        )
    }

    private func identifiers(_ issues: [IssueEntity]) -> [String] {
        issues.compactMap(\.identifier)
    }

    // Only the INVERSE side of a `blocks` row counts: a row whose
    // `related_issue_id` is me means I am blocked. The forward side (I block
    // something else) and every other type never gate a start.
    func testCountsOnlyBlockedByRelations() {
        let me = issue("me", identifier: "EXP-3")
        let blocker = issue("low", identifier: "EXP-1")
        let blocked = issue("high", identifier: "EXP-9")
        let parent = issue("parent", identifier: "EXP-2")
        let related = issue("related", identifier: "EXP-4")
        let blockers = StackStart.openBlockers(
            issueId: "me",
            relations: [
                relation("r1", from: "low", to: "me", type: "blocks"),
                // I block EXP-9 — not my problem.
                relation("r2", from: "me", to: "high", type: "blocks"),
                relation("r3", from: "parent", to: "me", type: "parent"),
                relation("r4", from: "related", to: "me", type: "related"),
            ],
            issues: [me, blocker, blocked, parent, related]
        )
        XCTAssertEqual(identifiers(blockers), ["EXP-1"])
    }

    // A finished blocker blocks nothing — the three terminal anchors drop out,
    // the live ones stay, ordered by identifier.
    func testDropsABlockerThatIsDoneCancelledOrADuplicate() {
        let me = issue("me", identifier: "EXP-9")
        let done = issue("a", identifier: "EXP-1", status: "done")
        let cancelled = issue("b", identifier: "EXP-2", status: "cancelled")
        let duplicate = issue("c", identifier: "EXP-3", status: "duplicate")
        let review = issue("d", identifier: "EXP-5", status: "in_review")
        let open = issue("e", identifier: "EXP-4", status: "in_progress")
        let blockers = StackStart.openBlockers(
            issueId: "me",
            relations: [
                relation("r1", from: "a", to: "me", type: "blocks"),
                relation("r2", from: "b", to: "me", type: "blocks"),
                relation("r3", from: "c", to: "me", type: "blocks"),
                relation("r4", from: "d", to: "me", type: "blocks"),
                relation("r5", from: "e", to: "me", type: "blocks"),
            ],
            issues: [me, done, cancelled, duplicate, review, open]
        )
        XCTAssertEqual(identifiers(blockers), ["EXP-4", "EXP-5"])
    }

    // A blocker whose issue row has not synced cannot be named, ordered or
    // stacked on — it is not a blocker here.
    func testDropsABlockerWhoseIssueRowIsNotSynced() {
        let me = issue("me", identifier: "EXP-9")
        let known = issue("known", identifier: "EXP-1")
        let blockers = StackStart.openBlockers(
            issueId: "me",
            relations: [
                relation("r1", from: "known", to: "me", type: "blocks"),
                relation("r2", from: "ghost", to: "me", type: "blocks"),
            ],
            issues: [me, known]
        )
        XCTAssertEqual(identifiers(blockers), ["EXP-1"])
    }

    // The copy is byte-locked: every client says the same thing.
    func testCopyIsByteLocked() {
        XCTAssertEqual(StackStart.blockedStartTitle, "This issue is blocked")
        XCTAssertEqual(StackStart.startAnywayLabel, "Start anyway")
        XCTAssertEqual(StackStart.stackedPrLabel, "Stacked PR")
        XCTAssertEqual(StackStart.bodyPrefix, "This issue is blocked by ")
        XCTAssertEqual(StackStart.bodySuffix, ". Start anyway, or start a stacked PR?")
    }
}
