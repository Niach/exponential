import Foundation
import SwiftUI
import XCTest
import ExpUI

/// #EXP-2109 (chats.json `shell.dock`, `shell.newMenu`): the docked bar; and
/// `shell.bar`, a running app's screens capsule: label-sized tabs that share
/// the rest, truncating only when the labels alone do not fit.
final class MobileTabBarMetricsTests: XCTestCase {

    func testTheDockReadsTheFixture() {
        XCTAssertEqual(PhoneDockMetrics.height, 60)
        XCTAssertEqual(PhoneDockMetrics.icon, 22)
        XCTAssertEqual(PhoneDockMetrics.label, 11)
        XCTAssertEqual(PhoneDockMetrics.gap, 4)
        XCTAssertEqual(PhoneDockMetrics.disc, 60)
        XCTAssertEqual(PhoneDockMetrics.mark, 50)
        XCTAssertEqual(PhoneDockMetrics.raise, 16)
        XCTAssertEqual(PhoneDockMetrics.ring, 4)
        XCTAssertEqual(PhoneDockMetrics.glow, 26)
        XCTAssertEqual(PhoneDockMetrics.clearance, 82)
        XCTAssertEqual(PhoneDockMetrics.menuWidth, 160)
        XCTAssertEqual(PhoneDockMetrics.menuRowHeight, 48)
        XCTAssertEqual(PhoneDockMetrics.menuRadius, 20)
        XCTAssertEqual(PhoneDockMetrics.menuGap, 26)
    }

    /// Five equal cells stay over the 44pt target on the narrowest phone.
    func testADockCellPassesTheHigMinimum() {
        XCTAssertGreaterThanOrEqual(320 / CGFloat(5), 44)
        XCTAssertGreaterThanOrEqual(PhoneDockMetrics.height, 44)
        XCTAssertGreaterThanOrEqual(PhoneDockMetrics.menuRowHeight, 44)
    }

    func testTheBarReadsTheFixture() {
        XCTAssertEqual(MobileTabBarMetrics.barHeight, 52)
        XCTAssertEqual(MobileTabBarMetrics.pillPadding, 4)
        XCTAssertEqual(MobileTabBarMetrics.tabHeight, 44)
        XCTAssertEqual(MobileTabBarMetrics.tabMinWidth, 44)
        XCTAssertEqual(MobileTabBarMetrics.tabPaddingX, 8)
        XCTAssertEqual(MobileTabBarMetrics.lensScale, 1.12, accuracy: 0.0001)
        XCTAssertEqual(MobileTabBarMetrics.cancelDistance, 56)
    }

    func testANaturalWidthIsTheLabelPlusPaddingAtLeastTheMinimum() {
        XCTAssertEqual(MobileTabBarMetrics.naturalTabWidth(label: 60), 76)
        XCTAssertEqual(MobileTabBarMetrics.naturalTabWidth(label: 10), 44)
    }

    func testTheTabsShareTheRestEvenly() {
        let widths = MobileTabBarMetrics.tabWidths(natural: [76, 44, 44, 50], available: 234)
        XCTAssertEqual(widths, [81, 49, 49, 55])
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
}
