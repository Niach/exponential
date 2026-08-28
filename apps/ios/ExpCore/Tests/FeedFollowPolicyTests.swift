import Foundation
import XCTest

@testable import ExpCore

// EXP-656: the agent feed's follow pin. Geometry alone can't say why the
// bottom edge moved — the rule separates a reader reaching the bottom from
// content resizing under one who never touched the screen.
final class FeedFollowPolicyTests: XCTestCase {

    func testReachingTheBottomRearms() {
        XCTAssertEqual(
            FeedFollowPolicy.decide(
                pinned: true, atBottom: false, userScrolling: true,
                offsetDelta: 120, heightDelta: 0
            ),
            .rearm
        )
    }

    // The EXP-656 case: a staged replay commits a shorter history, the content
    // shrinks and the bottom edge arrives at a stationary reader. Follow must
    // NOT re-arm — that is the yank out of the plan they were reading.
    func testShrinkingContentUnderAStationaryViewerHolds() {
        XCTAssertEqual(
            FeedFollowPolicy.decide(
                pinned: true, atBottom: false, userScrolling: false,
                offsetDelta: 0, heightDelta: -800
            ),
            .hold
        )
    }

    // Shrinking WHILE the reader is scrolling is still their gesture landing at
    // the bottom, so it re-arms like any other arrival.
    func testShrinkingDuringAUserScrollStillRearms() {
        XCTAssertEqual(
            FeedFollowPolicy.decide(
                pinned: true, atBottom: false, userScrolling: true,
                offsetDelta: 40, heightDelta: -800
            ),
            .rearm
        )
    }

    // Already following: nothing to do, and no state write per sample.
    func testAPinnedFollowedFeedHolds() {
        XCTAssertEqual(
            FeedFollowPolicy.decide(
                pinned: true, atBottom: true, userScrolling: false,
                offsetDelta: 0, heightDelta: 200
            ),
            .hold
        )
    }

    func testADragUpUnpins() {
        XCTAssertEqual(
            FeedFollowPolicy.decide(
                pinned: false, atBottom: true, userScrolling: true,
                offsetDelta: -60, heightDelta: 0
            ),
            .unpin
        )
    }

    // Content GROWING un-pins the geometry with no gesture — the growth chaser
    // re-pins instead (EXP-272/EXP-306), so the pin itself must not drop.
    func testGrowthUnderAStationaryViewerHolds() {
        XCTAssertEqual(
            FeedFollowPolicy.decide(
                pinned: false, atBottom: true, userScrolling: false,
                offsetDelta: 0, heightDelta: 400
            ),
            .hold
        )
    }

    // A finger resting at the bottom while content grows: a scroll phase is
    // active but the offset never moved up (EXP-306).
    func testGrowthUnderARestingFingerHolds() {
        XCTAssertEqual(
            FeedFollowPolicy.decide(
                pinned: false, atBottom: true, userScrolling: true,
                offsetDelta: 0, heightDelta: 400
            ),
            .hold
        )
    }

    // Already un-pinned: scrolling further up changes nothing.
    func testScrollingUpWhileUnpinnedHolds() {
        XCTAssertEqual(
            FeedFollowPolicy.decide(
                pinned: false, atBottom: false, userScrolling: true,
                offsetDelta: -300, heightDelta: 0
            ),
            .hold
        )
    }
}
