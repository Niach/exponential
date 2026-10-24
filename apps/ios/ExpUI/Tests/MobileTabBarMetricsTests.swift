import Foundation
import SwiftUI
import XCTest
import ExpUI

/// #EXP-1384 (chats.json `shell.bar`): Board · Chats · Apps · Team on a
/// capsule that takes the width beside the launcher, label-sized tabs that
/// share the rest, truncating only when the labels alone do not fit.
final class MobileTabBarMetricsTests: XCTestCase {

    func testTheBarReadsTheFixture() {
        XCTAssertEqual(MobileTabBarMetrics.barHeight, 52)
        XCTAssertEqual(MobileTabBarMetrics.pillPadding, 4)
        XCTAssertEqual(MobileTabBarMetrics.tabHeight, 44)
        XCTAssertEqual(MobileTabBarMetrics.launcherGap, 12)
        XCTAssertEqual(MobileTabBarMetrics.tabMinWidth, 44)
        XCTAssertEqual(MobileTabBarMetrics.tabPaddingX, 8)
        XCTAssertEqual(MobileTabBarMetrics.lensScale, 1.12, accuracy: 0.0001)
        XCTAssertEqual(MobileTabBarMetrics.cancelDistance, 56)
        XCTAssertEqual(MobileTabBarMetrics.caretSize, 10)
    }

    func testTheTouchTargetsPassTheHigMinimum() {
        XCTAssertGreaterThanOrEqual(MobileTabBarMetrics.tabHeight, 44)
        XCTAssertGreaterThanOrEqual(MobileTabBarMetrics.tabMinWidth, 44)
        XCTAssertEqual(MobileTabBarMetrics.launcherArm, FloatingBarTokens.slot)
    }

    func testANaturalWidthIsTheLabelPlusPaddingAtLeastTheMinimum() {
        XCTAssertEqual(MobileTabBarMetrics.naturalTabWidth(label: 60), 76)
        XCTAssertEqual(MobileTabBarMetrics.naturalTabWidth(label: 10), 44)
    }

    func testTheTabsShareTheRestEvenly() {
        // 375 − 2×16 − 12 − 104.5 = 226.5, less the capsule padding = 218.5.
        let capsule = MobileTabBarMetrics.capsuleTabsWidth(screenWidth: 375)
        XCTAssertEqual(capsule, 226.5)
        let available = capsule - 2 * MobileTabBarMetrics.pillPadding
        let widths = MobileTabBarMetrics.tabWidths(natural: [76, 44, 44, 50], available: available)
        XCTAssertEqual(widths.reduce(0, +), available, accuracy: 0.001)
        let extra = (available - 214) / 4
        XCTAssertEqual(widths, [76 + extra, 44 + extra, 44 + extra, 50 + extra])
    }

    func testTheWidestGivesWayFirstWhenTheLabelsDoNotFit() {
        let widths = MobileTabBarMetrics.tabWidths(natural: [140, 50, 44, 90], available: 220)
        // 50 + 44 fit whole; the two widest share the 126 left: 63 each.
        XCTAssertEqual(widths, [63, 50, 44, 63])
        XCTAssertEqual(widths.reduce(0, +), 220, accuracy: 0.001)
    }

    func testATabNeverShrinksBelowTheMinimum() {
        let widths = MobileTabBarMetrics.tabWidths(natural: [120, 120, 120, 120], available: 100)
        XCTAssertEqual(widths, [44, 44, 44, 44])
    }

    func testTheBoardMenuHeightCapsAtTheVisibleRows() {
        XCTAssertEqual(MobileTabBarMetrics.menuHeight(rows: 4), 4 * 48 + 8)
        XCTAssertEqual(MobileTabBarMetrics.menuHeight(rows: 20), 8 * 48 + 8)
        XCTAssertEqual(MobileTabBarMetrics.menuRadius, 28)
    }
}
