import Foundation
import XCTest
@testable import ExpCore

// EXP-1245: the owner's turn facts off the relay feed — observed turn edges,
// placed messages, and the run's start opening the first turn. Test names
// mirror web `session-turn-events.test.ts`.
final class SessionTurnEventsTests: XCTestCase {
    private let start: Double = 1_000

    private func message(_ id: Int, _ text: String, subagentId: String? = nil) -> SessionTurnLog.Row {
        SessionTurnLog.Row(id: id, isUserMessage: true, text: text, subagentId: subagentId)
    }

    private func narration(_ id: Int) -> SessionTurnLog.Row {
        SessionTurnLog.Row(id: id, isUserMessage: false)
    }

    func testYieldsNothingWhileOnlyTheRunsStartIsKnown() {
        let log = SessionTurnLog()
        log.recordTurnSlot(.ended, startedAt: nil, now: 2_000)
        log.recordFeedMessages([], now: 2_000)
        XCTAssertEqual(log.turnEvents([], runStartedAt: start), [])
    }

    func testRecordsAStartedEdgeAndAnEndedEdgeSeenToFollowIt() {
        let log = SessionTurnLog()
        log.recordTurnSlot(.started, startedAt: 5_000, now: 5_100)
        log.recordTurnSlot(.started, startedAt: 5_000, now: 6_000)
        log.recordTurnSlot(.ended, startedAt: 5_000, now: 9_000)
        XCTAssertEqual(log.edges, [.turn(started: true, at: 5_000), .turn(started: false, at: 9_000)])
    }

    func testNeverInventsAnEndForASlotAlreadyEndedOnArrival() {
        let log = SessionTurnLog()
        log.recordTurnSlot(.ended, startedAt: 5_000, now: 9_000)
        XCTAssertEqual(log.edges, [])
    }

    func testPlacesALiveArrivalByNowNeverAReplaysBulk() {
        let log = SessionTurnLog()
        let replay = [message(1, "old one"), narration(2), message(3, "old two")]
        log.recordFeedMessages(replay, now: 2_000)
        XCTAssertTrue(log.messageAt.isEmpty)
        let live = replay + [message(4, "follow-up")]
        log.recordFeedMessages(live, now: 7_000)
        XCTAssertEqual(log.messageAt, [4: 7_000])
    }

    func testSkipsASubagentsMessage() {
        let log = SessionTurnLog()
        log.recordFeedMessages([], now: 1)
        log.recordFeedMessages([message(1, "hi", subagentId: "s1")], now: 2)
        XCTAssertTrue(log.messageAt.isEmpty)
    }

    func testFeedsSessionTurnsTwoTurnsWithThePersonsBubbleBetween() {
        let log = SessionTurnLog()
        log.recordFeedMessages([], now: start)
        log.recordTurnSlot(.started, startedAt: start, now: start)
        log.recordTurnSlot(.ended, startedAt: start, now: 4_000)
        let feed = [message(1, "stack it\n![image](/api/attachments/i9)")]
        log.recordFeedMessages(feed, now: 5_000)
        log.recordTurnSlot(.started, startedAt: 5_100, now: 5_100)
        let results = """
            [{"topic":"Summary","text":"first","at":3000},{"topic":"Reviews","text":"second","at":6000}]
            """
        let turns = sessionTurns(results, feed: log.turnEvents(feed, runStartedAt: start))
        XCTAssertTrue(turns.perTurn)
        XCTAssertEqual(turns.turns.count, 2)
        XCTAssertNil(turns.turns[0].message)
        XCTAssertEqual(turns.turns[0].endedAt, 4_000)
        XCTAssertEqual(turns.turns[0].reply, "first")
        XCTAssertEqual(turns.turns[1].message?.text, "stack it")
        XCTAssertEqual(turns.turns[1].message?.images, ["/api/attachments/i9"])
        XCTAssertEqual(turns.turns[1].startedAt, 5_100)
        XCTAssertNil(turns.turns[1].endedAt)
        XCTAssertEqual(turns.turns[1].items, [.text(topic: "Reviews", text: "second")])
    }

    func testTheBubbleCaptionDropsMissingParts() {
        let utc = TimeZone(identifier: "UTC")!
        // 2025-10-09T21:40:00Z
        let at: Double = 1_760_046_000_000
        XCTAssertEqual(userMessageTime(at, timeZone: utc), "21:40")
        XCTAssertEqual(
            userMessageCaption(name: "Danny", at: at, device: "mint", timeZone: utc),
            "Danny · 21:40 · from mint"
        )
        XCTAssertEqual(userMessageCaption(name: " ", at: at, device: nil, timeZone: utc), "21:40")
        XCTAssertEqual(userMessageCaption(name: "Danny", at: nil, device: nil), "Danny")
        XCTAssertEqual(userMessageCaption(name: nil, at: nil, device: nil), "")
    }
}
