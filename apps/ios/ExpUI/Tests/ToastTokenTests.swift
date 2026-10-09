import CoreGraphics
import ExpCore
import XCTest
@testable import ExpUI
import SwiftUI

// EXP-1031: the iOS toast host's placement rules pin to the shared contract
// (`ToastStack.Constants`, packages/domain-contract/fixtures/toast-stack.json).
final class ToastTokenTests: XCTestCase {

    func testCardRadiusIsRadiusLg() {
        XCTAssertEqual(ToastHostMetrics.cardRadius, DesignTokens.Radius.lg)
        XCTAssertEqual(ToastHostMetrics.cardRadius, 12)
    }

    func testInsetsAreTheMobileViewportOffset() {
        XCTAssertEqual(Double(ToastHostMetrics.screenInset), ToastStack.Constants.mobileViewportOffset)
        XCTAssertEqual(ToastHostMetrics.screenInset, 16)
    }

    func testTheHostHangsFromTheTop() {
        XCTAssertEqual(ToastHostMetrics.placement, "top-center")
        XCTAssertEqual(ToastHostMetrics.placement, ToastStack.Constants.placementTouch)
        XCTAssertFalse(ToastHostMetrics.anchoredBottom)
        XCTAssertEqual(ToastHostMetrics.alignment, .top)
        XCTAssertEqual(ToastHostMetrics.scaleAnchor, .bottom)
        XCTAssertEqual(ToastHostMetrics.transitionEdge, .top)
        // Collapsed from the top: the older cards peek out BELOW the front one.
        let collapsed = ToastStack.geometry(
            heights: [60, 60, 60], expanded: false, anchoredBottom: ToastHostMetrics.anchoredBottom
        )
        XCTAssertEqual(collapsed.items.map(\.offset), [28, 14, 0])
    }

    func testPhoneWidthIsFullWidthMinusTheInsets() {
        XCTAssertEqual(ToastHostMetrics.cardWidth(container: 393, regular: false), 393 - 32)
        XCTAssertEqual(ToastHostMetrics.cardWidth(container: 20, regular: false), 0)
    }

    func testRegularWidthKeepsThePointerWidth() {
        XCTAssertEqual(Double(ToastHostMetrics.regularWidth), ToastStack.Constants.width)
        XCTAssertEqual(ToastHostMetrics.cardWidth(container: 1024, regular: true), 356)
        XCTAssertEqual(ToastHostMetrics.cardWidth(container: 300, regular: true), 268)
    }

    /// Safe-area top (the overlay window's own layout) + 16, plus the root
    /// status banner's height while one is up.
    func testTopPaddingIsTheViewportOffsetBelowTheBanner() {
        XCTAssertEqual(ToastHostMetrics.topPadding(bannerHeight: 0), 16)
        XCTAssertEqual(ToastHostMetrics.topPadding(bannerHeight: 24), 40)
        XCTAssertEqual(ToastHostMetrics.topPadding(bannerHeight: -5), 16)
    }

    func testSwipeDismissesPastTheThresholdOnly() {
        XCTAssertEqual(Double(ToastHostMetrics.swipeThreshold), ToastStack.Constants.swipeThreshold)
        XCTAssertFalse(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 45, height: 0)))
        XCTAssertTrue(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 46, height: 0)))
        XCTAssertTrue(ToastHostMetrics.dismissesOnSwipe(CGSize(width: -46, height: 0)))
        XCTAssertTrue(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 0, height: -46)))
        XCTAssertFalse(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 0, height: -45)))
        XCTAssertFalse(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 0, height: 200)))
    }

    func testDownwardDragsClamp() {
        XCTAssertEqual(ToastHostMetrics.clampedDrag(CGSize(width: 10, height: 30)), CGSize(width: 10, height: 0))
        XCTAssertEqual(ToastHostMetrics.clampedDrag(CGSize(width: -10, height: -30)), CGSize(width: -10, height: -30))
    }

    func testEveryKindHasAConceptIcon() {
        XCTAssertEqual(ToastKind.success.iconName, AppIcons.uiSuccess)
        XCTAssertEqual(ToastKind.error.iconName, AppIcons.uiError)
        XCTAssertEqual(ToastKind.info.iconName, AppIcons.uiInfo)
        XCTAssertEqual(ToastKind.warning.iconName, AppIcons.uiWarning)
    }

    @MainActor
    func testExpandingPausesAndAnEmptyStackCannotExpand() {
        let toaster = Toaster()
        toaster.expanded = true
        XCTAssertFalse(toaster.expanded)
        toaster.error("Nope")
        toaster.expanded = true
        XCTAssertTrue(toaster.expanded)
        toaster.dismiss(toaster.items[0].id)
        XCTAssertTrue(toaster.items.isEmpty)
        XCTAssertFalse(toaster.expanded)
    }
}
