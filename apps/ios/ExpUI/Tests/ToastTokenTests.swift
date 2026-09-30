import CoreGraphics
import ExpCore
import XCTest
@testable import ExpUI

// EXP-1031: the iOS toast host's placement rules pin to the shared contract
// (`ToastStack.Constants`, packages/domain-contract/fixtures/toast-stack.json).
final class ToastTokenTests: XCTestCase {

    func testCardRadiusIsRadiusLg() {
        XCTAssertEqual(ToastHostMetrics.cardRadius, DesignTokens.Radius.lg)
        XCTAssertEqual(ToastHostMetrics.cardRadius, 12)
    }

    func testInsetsAreTheMobileViewportOffset() {
        XCTAssertEqual(Double(ToastHostMetrics.screenInset), ToastStack.Constants.mobileViewportOffset)
        XCTAssertEqual(ToastHostMetrics.tabBarClearance, 80)
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

    func testBottomPaddingClearsTheBarOrTheViewportOffset() {
        XCTAssertEqual(ToastHostMetrics.bottomPadding(bottomInset: 0), ToastHostMetrics.screenInset)
        XCTAssertEqual(
            ToastHostMetrics.bottomPadding(bottomInset: ToastHostMetrics.tabBarClearance),
            ToastHostMetrics.tabBarClearance
        )
    }

    func testSwipeDismissesPastTheThresholdOnly() {
        XCTAssertEqual(Double(ToastHostMetrics.swipeThreshold), ToastStack.Constants.swipeThreshold)
        XCTAssertFalse(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 45, height: 0)))
        XCTAssertTrue(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 46, height: 0)))
        XCTAssertTrue(ToastHostMetrics.dismissesOnSwipe(CGSize(width: -46, height: 0)))
        XCTAssertTrue(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 0, height: 46)))
        XCTAssertFalse(ToastHostMetrics.dismissesOnSwipe(CGSize(width: 0, height: -200)))
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
