import Foundation
import SwiftUI
import XCTest
import ExpUI

/// Thread 58cb32df (chats.json `shell`): four labelled tabs (Board · Devices
/// · Chats · Team) beside the split launcher, which rides EVERY bar-visible
/// route (EXP-973), have to fit the narrowest phone.
final class MobileTabBarMetricsTests: XCTestCase {

    func testFourTabsAndTheFullCapsuleFitTheSmallestPhone() {
        let width = MobileTabBarMetrics.barWidth(
            tabs: 4, launcher: MobileTabBarMetrics.capsuleWidth
        )
        // 12 + (4×56 + 0 + 8) + 8 + 104.5 + 12 = 368.5
        XCTAssertEqual(width, 368.5)
        XCTAssertLessThanOrEqual(width, MobileTabBarMetrics.smallestPhoneWidth)
    }

    func testTheLoneCircleIsNeverTheTighterCase() {
        XCTAssertLessThan(
            MobileTabBarMetrics.barWidth(tabs: 4, launcher: MobileTabBarMetrics.circleWidth),
            MobileTabBarMetrics.barWidth(tabs: 4, launcher: MobileTabBarMetrics.capsuleWidth)
        )
    }

    func testTheTouchTargetsPassTheHigMinimum() {
        XCTAssertGreaterThanOrEqual(MobileTabBarMetrics.tabHeight, 44)
        XCTAssertGreaterThanOrEqual(MobileTabBarMetrics.tabWidth, 44)
        XCTAssertEqual(MobileTabBarMetrics.launcherArm, FloatingBarTokens.slot)
        XCTAssertEqual(MobileTabBarMetrics.barHeight, 60)
    }
}
