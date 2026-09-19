import XCTest
@testable import ExpCore

/// EXP-935: the hold that keeps a switched / resumed run's screen from popping
/// to the list before its successor lands.
final class ContinuationHoldTests: XCTestCase {

    func testNothingIsPendingAtRest() {
        let hold = ContinuationHold()
        XCTAssertEqual(hold.state, .idle)
        XCTAssertFalse(hold.isPending)
        XCTAssertNil(hold.landedId)
    }

    func testTheCommandHoldsFromTheMomentItIsSent() {
        var hold = ContinuationHold()
        hold.sending()
        XCTAssertTrue(hold.isPending)
        let moment = Date()
        hold.sent(at: moment)
        XCTAssertEqual(hold.state, .sent(at: moment))
        XCTAssertTrue(hold.isPending)
    }

    func testTheSuccessorEndsTheHoldAndNamesItself() {
        var hold = ContinuationHold()
        hold.sending()
        hold.sent()
        hold.landed("session-2")
        XCTAssertEqual(hold.landedId, "session-2")
        XCTAssertFalse(hold.isPending)
    }

    func testTheDeadlineExpiresAHoldNoRowEverAnswered() {
        var hold = ContinuationHold()
        let moment = Date()
        hold.sending()
        hold.sent(at: moment)
        hold.tick(now: moment.addingTimeInterval(ContinuationHold.deadline - 1))
        XCTAssertTrue(hold.isPending)
        hold.tick(now: moment.addingTimeInterval(ContinuationHold.deadline))
        XCTAssertEqual(hold.state, .expired)
        XCTAssertFalse(hold.isPending)
    }

    func testARefusedSendStopsHoldingButALandedOneIsNeverUnlanded() {
        var hold = ContinuationHold()
        hold.sending()
        hold.expire()
        XCTAssertEqual(hold.state, .expired)

        var landed = ContinuationHold()
        landed.sending()
        landed.landed("session-2")
        landed.expire()
        landed.tick(now: Date().addingTimeInterval(ContinuationHold.deadline * 2))
        XCTAssertEqual(landed.landedId, "session-2")
    }

    func testTheHoldRunsTheWatchersOwnDeadline() {
        XCTAssertEqual(ContinuationHold.deadline, StartedRunMatch.deadline)
    }
}
