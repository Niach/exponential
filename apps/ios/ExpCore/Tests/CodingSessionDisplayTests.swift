import Foundation
import XCTest
@testable import ExpCore

// EXP-358: the `merged` session status IS the display state — a PR merge no
// longer ends the run. The pre-EXP-358 `in_review` + merged-PR derivation has
// to keep working for rows written by older servers/desktops.
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

    func testMergedStatusWins() {
        XCTAssertEqual(
            CodingSessionDisplayState.of(session: session(status: "merged"), prState: "merged"),
            .merged
        )
        // The status stands on its own — the issue's prState may not have
        // synced yet, and a batch/action session has no issue at all.
        XCTAssertEqual(
            CodingSessionDisplayState.of(session: session(status: "merged"), prState: nil),
            .merged
        )
    }

    func testMergedOutranksNeedsInput() {
        // Same precedence the PR-derived merge already had: a merged run is
        // not waiting on a picker.
        XCTAssertEqual(
            CodingSessionDisplayState.of(
                session: session(status: "merged", needsInput: true), prState: "open"
            ),
            .merged
        )
    }

    func testLegacyInReviewWithMergedPrStaysDone() {
        // Old rows never get the new status — the PR outcome still resolves
        // them (EXP-214).
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
