import Foundation
import XCTest
@testable import ExpCore

// EXP-540: a PR merge ends the run (EXP-498), so no session status of its own
// carries "merged". The `in_review` + merged-PR derivation has to keep working
// for rows a lagging older server left parked in review.
final class CodingSessionDisplayTests: XCTestCase {
    private func session(status: String, needsInput: Bool = false) -> CodingSessionEntity {
        CodingSessionEntity(
            id: "sess-1",
            issueId: "issue-1",
            boardId: nil,
            teamId: "ws-1",
            userId: "user-1",
            deviceLabel: nil,
            status: status,
            needsInput: needsInput,
            startedAt: "2026-07-17T09:00:00Z",
            endedAt: nil,
            createdAt: "2026-07-17T09:00:00Z",
            updatedAt: "2026-07-17T11:30:00Z"
        )
    }

    func testInReviewWithMergedPrIsDone() {
        // Old-server tolerance: the row never left `in_review` after its PR
        // merged, so the PR outcome resolves it (EXP-214).
        XCTAssertEqual(
            CodingSessionDisplayState.of(session: session(status: "in_review"), prState: "merged"),
            .done
        )
    }

    func testInReviewWithOpenPrIsReview() {
        XCTAssertEqual(
            CodingSessionDisplayState.of(session: session(status: "in_review"), prState: "open"),
            .review
        )
    }

    func testInReviewOutranksNeedsInput() {
        // EXP-531: the PR is open — the run is done coding. A stale
        // needs_input (claude's idle nudge, forwarded by the desktop after
        // the turn ended) must not mask "Ready for review".
        XCTAssertEqual(
            CodingSessionDisplayState.of(
                session: session(status: "in_review", needsInput: true), prState: "open"
            ),
            .review
        )
    }

    func testNeedsInputAndRunning() {
        XCTAssertEqual(
            CodingSessionDisplayState.of(
                session: session(status: "running", needsInput: true), prState: nil
            ),
            .needsInput
        )
        XCTAssertEqual(
            CodingSessionDisplayState.of(session: session(status: "running"), prState: nil),
            .running
        )
    }
}
