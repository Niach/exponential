import XCTest
@testable import ExpCore

// #EXP-2004: the New chat shortcut focuses the composer on a cold start too.
final class ComposerFocusRequestTests: XCTestCase {
    func testWarmStartSpendsOnTheBoundComposer() {
        XCTAssertTrue(ComposerFocusRequest.shouldSpend(pending: true, newChat: true, composerTeamId: "t1", pageTeamId: "t1"))
    }

    func testColdStartWaitsForTheTeamsComposer() {
        // The first composer mounts before the team resolves…
        XCTAssertFalse(ComposerFocusRequest.shouldSpend(pending: true, newChat: true, composerTeamId: nil, pageTeamId: nil))
        // …the team arrives, the old composer is about to be rebuilt…
        XCTAssertFalse(ComposerFocusRequest.shouldSpend(pending: true, newChat: true, composerTeamId: nil, pageTeamId: "t1"))
        // …and the rebuilt one takes the focus.
        XCTAssertTrue(ComposerFocusRequest.shouldSpend(pending: true, newChat: true, composerTeamId: "t1", pageTeamId: "t1"))
    }

    func testNothingPendingOrPushedSpendsNothing() {
        XCTAssertFalse(ComposerFocusRequest.shouldSpend(pending: false, newChat: true, composerTeamId: "t1", pageTeamId: "t1"))
        XCTAssertFalse(ComposerFocusRequest.shouldSpend(pending: true, newChat: false, composerTeamId: "t1", pageTeamId: "t1"))
    }
}
