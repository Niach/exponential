import Foundation
import SwiftUI
import XCTest
import ExpUI

// EXP-893: `FloatingBottomBar` is the ONE floating bar — the issue detail
// bar, the tab bar's launcher circle and the review bar all used to draw
// their own copy of the same 52pt chrome. These numbers are what keeps the
// Work screen's bar from jumping between faces: every slot is one height,
// the capsule's content band plus its inner padding IS that height, and the
// badge sits where the tab bar's dot sits.
final class FloatingBottomBarTokenTests: XCTestCase {

    func testEverySlotIsTheSharedFiftyTwoPointRung() {
        XCTAssertEqual(FloatingBarTokens.slot, 52)
        // The capsule reaches the slot height through its inner padding, so
        // it lines up with the circles beside it without a frame of its own.
        XCTAssertEqual(
            FloatingBarTokens.capsuleContentHeight + 2 * FloatingBarTokens.capsuleInnerPadding,
            FloatingBarTokens.slot
        )
    }

    // EXP-916: Android's `FloatingBottomBar` numbers — 20 inset, 10 between
    // the slots (12 in the review page's centred cluster), an 18-wide label
    // inset on the capsule, 28 on the solid pill, 20pt glyphs.
    func testSpacingAndInsetsAreAndroids() {
        XCTAssertEqual(FloatingBarTokens.spacing, 10)
        XCTAssertEqual(FloatingBarTokens.clusterSpacing, 12)
        XCTAssertEqual(FloatingBarTokens.inset, 20)
        XCTAssertEqual(FloatingBarTokens.topPadding, 8)
        XCTAssertEqual(FloatingBarTokens.bottomPadding, 4)
        XCTAssertEqual(
            FloatingBarTokens.capsuleHorizontalPadding + FloatingBarTokens.capsuleInnerPadding,
            18
        )
        XCTAssertEqual(FloatingBarTokens.capsuleGap, 8)
        XCTAssertEqual(FloatingBarTokens.solidPillHorizontalPadding, 28)
        XCTAssertEqual(FloatingBarTokens.glyph, AppIcon.Size.large)
    }

    // The badge is the tab bar's 8pt disc, re-based on the 52pt square.
    func testBadgeIsTheTabBarsDot() {
        XCTAssertEqual(FloatingBarTokens.badgeSize, 8)
        XCTAssertEqual(FloatingBarTokens.badgeOffset, 12)
        XCTAssertLessThan(FloatingBarTokens.badgeSize, FloatingBarTokens.slot / 4)
    }
}
