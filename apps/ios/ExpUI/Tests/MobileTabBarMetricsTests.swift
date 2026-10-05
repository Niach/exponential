import Foundation
import SwiftUI
import XCTest
import ExpUI

/// EXP-973: the New-issue circle rides EVERY bar-visible route and the Agent
/// page is the bar's first tab, so the bar's widest arrangement — six tabs
/// beside the circle — has to fit the narrowest phone. These are the numbers
/// that keep it there.
final class MobileTabBarMetricsTests: XCTestCase {

    func testSixTabsAndTheCircleFitTheSmallestPhone() {
        let width = MobileTabBarMetrics.barWidth(
            tabs: 6, launcher: MobileTabBarMetrics.circleWidth
        )
        // 12 + (6×44 + 5×2 + 6) + 8 + 52 + 12 = 364
        XCTAssertEqual(width, 364)
        XCTAssertLessThanOrEqual(width, MobileTabBarMetrics.smallestPhoneWidth)
    }

    func testFiveTabsFitToo() {
        // EXP-1105: yolo mode (no open PR) hides Reviews, leaving five.
        let width = MobileTabBarMetrics.barWidth(
            tabs: 5, launcher: MobileTabBarMetrics.circleWidth
        )
        // 12 + (5×44 + 4×2 + 6) + 8 + 52 + 12 = 318
        XCTAssertEqual(width, 318)
        XCTAssertLessThanOrEqual(width, MobileTabBarMetrics.smallestPhoneWidth)
    }

    func testTheTouchTargetsAreTheHigMinimum() {
        XCTAssertEqual(MobileTabBarMetrics.tabHeight, 44)
        XCTAssertEqual(MobileTabBarMetrics.tabWidth, 44)
        XCTAssertEqual(MobileTabBarMetrics.circleWidth, FloatingBarTokens.slot)
    }
}
