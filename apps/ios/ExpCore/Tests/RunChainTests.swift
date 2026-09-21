import Foundation
import XCTest
@testable import ExpCore

// EXP-974: the resume chain (`RunChain.chain`) — the same six cases, by
// name, web (`run-chain.test.ts`), the desktop (`queries::run_chain` tests)
// and Android (`RunChainTest`) run.
final class RunChainTests: XCTestCase {

    private func row(_ id: String, _ resumedFromId: String?, _ createdAt: String) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: nil,
            teamId: "team-1",
            userId: "user-1",
            deviceLabel: "macbook",
            status: "ended",
            resumedFromId: resumedFromId,
            startedAt: createdAt,
            endedAt: nil,
            createdAt: createdAt,
            updatedAt: createdAt
        )
    }

    private func ids(_ rows: [CodingSessionEntity]) -> [String] { rows.map(\.id) }

    private lazy var a = row("a", nil, "2026-09-01T10:00:00Z")
    private lazy var b = row("b", "a", "2026-09-01T11:00:00Z")
    private lazy var c = row("c", "b", "2026-09-01T12:00:00Z")
    // An unrelated run of the same person, interleaved in time.
    private lazy var x = row("x", nil, "2026-09-01T11:30:00Z")

    func testReturnsJustTheSessionWhenNothingResumedItAndItResumedNothing() {
        XCTAssertEqual(ids(RunChain.chain([a, x], sessionId: "x")), ["x"])
    }

    func testWalksResumedFromIdBackwardsToTheFirstRow() {
        XCTAssertEqual(ids(RunChain.chain([c, x, a, b], sessionId: "c")), ["a", "b", "c"])
    }

    func testWalksForwardsToTheLatestResumeFromAMiddleOrFirstRow() {
        XCTAssertEqual(ids(RunChain.chain([c, x, a, b], sessionId: "a")), ["a", "b", "c"])
        XCTAssertEqual(ids(RunChain.chain([c, x, a, b], sessionId: "b")), ["a", "b", "c"])
    }

    func testFollowsTheNewestSuccessorAtAFork() {
        let c1 = row("c1", "b", "2026-09-01T12:00:00Z")
        let c2 = row("c2", "b", "2026-09-01T13:00:00Z")
        let d2 = row("d2", "c2", "2026-09-01T14:00:00Z")
        let rows = [a, b, c1, c2, d2]
        // From the root or the fork point: the newest branch, to its end.
        XCTAssertEqual(ids(RunChain.chain(rows, sessionId: "a")), ["a", "b", "c2", "d2"])
        XCTAssertEqual(ids(RunChain.chain(rows, sessionId: "b")), ["a", "b", "c2", "d2"])
        // From the older sibling: its own past plus itself — never the other
        // branch.
        XCTAssertEqual(ids(RunChain.chain(rows, sessionId: "c1")), ["a", "b", "c1"])
        // Same stamp: the id breaks the tie, so every client picks the same row.
        let c3 = row("c3", "b", "2026-09-01T13:00:00Z")
        XCTAssertEqual(ids(RunChain.chain([a, b, c2, c3], sessionId: "b")), ["a", "b", "c3"])
    }

    func testYieldsEmptyForAnUnknownId() {
        XCTAssertEqual(ids(RunChain.chain([a, b, c], sessionId: "nope")), [])
        XCTAssertEqual(ids(RunChain.chain([], sessionId: "a")), [])
    }

    func testToleratesAPredecessorTheSweepDeletedDanglingResumedFromId() {
        // `b` resumed `a`, but `a` is gone: the chain starts at `b`.
        XCTAssertEqual(ids(RunChain.chain([b, c], sessionId: "c")), ["b", "c"])
        XCTAssertEqual(ids(RunChain.chain([b, c], sessionId: "b")), ["b", "c"])
    }

    func testNeverLoopsOnACyclicResumedFromId() {
        let p = row("p", "q", "2026-09-01T10:00:00Z")
        let q = row("q", "p", "2026-09-01T11:00:00Z")
        XCTAssertEqual(ids(RunChain.chain([p, q], sessionId: "p")), ["q", "p"])
        XCTAssertEqual(ids(RunChain.chain([p, q], sessionId: "q")), ["p", "q"])
    }
}
