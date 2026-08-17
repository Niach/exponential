import Foundation
import SwiftUI
import XCTest
import ExpUI

// EXP-523: the shared motion tokens reach SwiftUI through `Motion`, whose one
// hard contract is that Reduce Motion yields `nil` — `withAnimation(nil)` and
// `.animation(nil, value:)` apply the state change instantly, which is what
// lets every call site drop its `if reduceMotion` branch.
final class MotionTests: XCTestCase {

    func testReduceMotionYieldsNoAnimation() {
        let motion = Motion(reduceMotion: true)
        XCTAssertNil(motion.fast)
        XCTAssertNil(motion.standard)
        XCTAssertNil(motion.slow)
        XCTAssertNil(motion.decelerate())
        XCTAssertNil(motion.accelerate())
        XCTAssertNil(motion.pulse(duration: 1.4))
    }

    func testNormalMotionYieldsAnAnimation() {
        let motion = Motion(reduceMotion: false)
        XCTAssertNotNil(motion.fast)
        XCTAssertNotNil(motion.standard)
        XCTAssertNotNil(motion.slow)
        XCTAssertNotNil(motion.decelerate())
        XCTAssertNotNil(motion.accelerate())
        XCTAssertNotNil(motion.pulse(duration: 1.4))
    }

    // The generated durations are seconds on this platform (tokens.json stores
    // milliseconds); a unit slip here would make every animation 1000x wrong.
    func testDurationsAreSecondsAndOrdered() {
        XCTAssertEqual(DesignTokens.Motion.Duration.fast, 0.120, accuracy: 0.0001)
        XCTAssertEqual(DesignTokens.Motion.Duration.standard, 0.180, accuracy: 0.0001)
        XCTAssertEqual(DesignTokens.Motion.Duration.slow, 0.280, accuracy: 0.0001)
    }

    // Control points are P1/P2 only — P0 = (0,0) and P3 = (1,1) are implicit,
    // and every curve must stay inside the unit square on x.
    func testEasingControlPointsAreInTheUnitSquareOnX() {
        for curve in [
            DesignTokens.Motion.Ease.standard,
            DesignTokens.Motion.Ease.decelerate,
            DesignTokens.Motion.Ease.accelerate,
        ] {
            XCTAssertTrue((0...1).contains(curve.x1))
            XCTAssertTrue((0...1).contains(curve.x2))
        }
    }
}
