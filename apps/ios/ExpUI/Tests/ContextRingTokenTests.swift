import Foundation
import SwiftUI
import XCTest
import ExpCore
import ExpUI

// EXP-893: the steer composer's usage ring. Its size is the footer's glyph
// rung, its stroke is a hairline pair, and its tone is the usage track's
// palette over the ×4-locked severity thresholds — so the ring and the
// Usage sheet it opens agree on what "nearly full" looks like.
final class ContextRingTokenTests: XCTestCase {

    func testRingGeometry() {
        XCTAssertEqual(ContextRingTokens.size, 16)
        XCTAssertEqual(ContextRingTokens.lineWidth, 2)
        XCTAssertEqual(ContextRingTokens.spacing, 6)
        // The stroke has to leave a hole, or it reads as a filled dot.
        XCTAssertLessThan(ContextRingTokens.lineWidth * 2, ContextRingTokens.size / 2)
    }

    func testToneFollowsTheUsageTrackPalette() {
        XCTAssertEqual(ContextRing.tone(.normal), GlassTokens.usageFill)
        XCTAssertEqual(ContextRing.tone(.warning), DesignTokens.Semantic.yellow)
        XCTAssertEqual(ContextRing.tone(.danger), DesignTokens.Semantic.red)
    }

    // An unknown share draws NO arc; a known one clamps to the unit interval.
    func testArcClampsAndTreatsUnknownAsEmpty() {
        XCTAssertEqual(ContextRing.arc(nil), 0)
        XCTAssertEqual(ContextRing.arc(-0.5), 0)
        XCTAssertEqual(ContextRing.arc(0.62), 0.62)
        XCTAssertEqual(ContextRing.arc(1.7), 1)
        XCTAssertEqual(ContextRing.arc(.nan), 0)
    }
}
