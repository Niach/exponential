import Foundation
import SwiftUI
import XCTest
import ExpUI

/// EXP-973: the split launcher rides EVERY bar-visible route, so the bar's
/// widest arrangement — five tabs beside the capsule (SLOP-4 retired the
/// sixth, Support) — has to fit the narrowest phone. These are the numbers
/// that keep it there.
final class MobileTabBarMetricsTests: XCTestCase {

    func testFiveTabsAndTheFullCapsuleFitTheSmallestPhone() {
        let width = MobileTabBarMetrics.barWidth(
            tabs: 5, launcher: MobileTabBarMetrics.capsuleWidth
        )
        // 12 + (5×44 + 4×2 + 6) + 8 + 104.5 + 12 = 370.5
        XCTAssertEqual(width, 370.5)
        XCTAssertLessThanOrEqual(width, MobileTabBarMetrics.smallestPhoneWidth)
    }

    func testFourTabsFitToo() {
        // EXP-1105: yolo mode (no open PR) hides Reviews, leaving four.
        let width = MobileTabBarMetrics.barWidth(
            tabs: 4, launcher: MobileTabBarMetrics.capsuleWidth
        )
        // 12 + (4×44 + 3×2 + 6) + 8 + 104.5 + 12 = 324.5
        XCTAssertEqual(width, 324.5)
        XCTAssertLessThanOrEqual(width, MobileTabBarMetrics.smallestPhoneWidth)
    }

    func testTheLoneCircleIsNeverTheTighterCase() {
        for tabs in 4...5 {
            XCTAssertLessThan(
                MobileTabBarMetrics.barWidth(tabs: tabs, launcher: MobileTabBarMetrics.circleWidth),
                MobileTabBarMetrics.barWidth(tabs: tabs, launcher: MobileTabBarMetrics.capsuleWidth)
            )
        }
    }

    func testTheTouchTargetsAreTheHigMinimum() {
        XCTAssertEqual(MobileTabBarMetrics.tabHeight, 44)
        XCTAssertEqual(MobileTabBarMetrics.tabWidth, 44)
        XCTAssertEqual(MobileTabBarMetrics.launcherArm, FloatingBarTokens.slot)
    }
}
