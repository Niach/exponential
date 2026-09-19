import Foundation
import SwiftUI
import XCTest
import ExpUI

/// EXP-973: the split launcher rides EVERY bar-visible route, so the bar's
/// widest arrangement — six tabs beside the capsule — has to fit the
/// narrowest phone. These are the numbers that keep it there.
final class MobileTabBarMetricsTests: XCTestCase {

    func testSixTabsAndTheFullCapsuleFitTheSmallestPhone() {
        let width = MobileTabBarMetrics.barWidth(
            tabs: 6, launcher: MobileTabBarMetrics.capsuleWidth
        )
        // 8 + (6×40 + 6) + 8 + 104.5 + 8 = 374.5
        XCTAssertEqual(width, 374.5)
        XCTAssertLessThanOrEqual(width, MobileTabBarMetrics.smallestPhoneWidth)
    }

    func testFiveTabsKeepTheRoomierSlotsAndStillFit() {
        let width = MobileTabBarMetrics.barWidth(
            tabs: 5, launcher: MobileTabBarMetrics.capsuleWidth
        )
        // 12 + (5×44 + 4×2 + 6) + 8 + 104.5 + 12 = 370.5
        XCTAssertEqual(width, 370.5)
        XCTAssertLessThanOrEqual(width, MobileTabBarMetrics.smallestPhoneWidth)
        XCTAssertEqual(MobileTabBarMetrics.tabWidth(tabs: 5), 44)
    }

    func testTheLoneCircleIsNeverTheTighterCase() {
        for tabs in 5...6 {
            XCTAssertLessThan(
                MobileTabBarMetrics.barWidth(tabs: tabs, launcher: MobileTabBarMetrics.circleWidth),
                MobileTabBarMetrics.barWidth(tabs: tabs, launcher: MobileTabBarMetrics.capsuleWidth)
            )
        }
    }

    func testTheTouchTargetsKeepTheirHeight() {
        // Only the WIDTH gives way on the crowded bar.
        XCTAssertEqual(MobileTabBarMetrics.tabHeight, 44)
        XCTAssertEqual(MobileTabBarMetrics.tabWidth(tabs: 6), 40)
        XCTAssertEqual(MobileTabBarMetrics.launcherArm, FloatingBarTokens.slot)
    }
}
