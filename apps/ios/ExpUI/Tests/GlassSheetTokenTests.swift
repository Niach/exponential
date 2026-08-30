import Foundation
import SwiftUI
import UIKit
import XCTest
import ExpUI

// EXP-687: a bottom sheet is a cross-client contract, not a per-platform taste
// call. Android pins the same numbers in
// `app/src/test/java/com/exponential/app/ui/components/GlassSheetDefaultsTest.kt`
// — this is its mirror, so a drift in either platform's chrome breaks a build
// instead of shipping two different-looking sheets.
//
// The "no ✕, no Cancel, one bottom button" rules are STRUCTURAL —
// `GlassSheetChrome` simply has no close control and exactly one action slot —
// so there is nothing here to assert; adding one would show up as a diff on
// this file's twin, the shell itself.
final class GlassSheetTokenTests: XCTestCase {

    // 24pt top corners (the `xl3` rung, web's `rounded-t-3xl`), not the system
    // default that reads as a different product.
    func testSheetGeometryMatchesTheAndroidDefaults() {
        XCTAssertEqual(GlassSheetTokens.cornerRadius, 24)
        XCTAssertEqual(GlassSheetTokens.cornerRadius, DesignTokens.Radius.xl3)
    }

    // A fitted sheet never claims more than 85 % of the screen — past that it
    // scrolls inside. Android's `FittedMaxHeightFraction` is the same number.
    func testFittedCapIsEightyFivePercent() {
        XCTAssertEqual(GlassSheetTokens.fittedMaxFraction, 0.85, accuracy: 0.0001)
    }

    // #18181B, OPAQUE and unblurred: a sheet is a surface, not a scrim. The
    // value is the shared `glass.backgroundBottom` token — never a literal.
    func testBackgroundIsTheOpaqueSharedGlassBottom() {
        let channels = self.channels(of: GlassSheetTokens.background)
        let tolerance = 1.0 / 255.0
        XCTAssertEqual(channels.red, 24.0 / 255.0, accuracy: tolerance, "red channel")
        XCTAssertEqual(channels.green, 24.0 / 255.0, accuracy: tolerance, "green channel")
        XCTAssertEqual(channels.blue, 27.0 / 255.0, accuracy: tolerance, "blue channel")
        XCTAssertEqual(channels.alpha, 1, accuracy: 0.0001, "a sheet's fill is opaque")
    }

    func testBackgroundIsTheSharedTokenItself() {
        let sheet = channels(of: GlassSheetTokens.background)
        let token = channels(of: DesignTokens.Glass.backgroundBottom)
        XCTAssertEqual(sheet.red, token.red, accuracy: 0.0001)
        XCTAssertEqual(sheet.green, token.green, accuracy: 0.0001)
        XCTAssertEqual(sheet.blue, token.blue, accuracy: 0.0001)
        XCTAssertEqual(sheet.alpha, token.alpha, accuracy: 0.0001)
    }

    // The header clears the system drag indicator (visible on EVERY sheet
    // since EXP-687), and the pinned action carries the same inset on both
    // platforms.
    func testHeaderAndActionPaddings() {
        XCTAssertEqual(GlassSheetTokens.headerTopPadding, 22)
        XCTAssertEqual(GlassSheetTokens.headerBottomPadding, 10)
        XCTAssertEqual(GlassSheetTokens.headerHPadding, 20)
        XCTAssertEqual(GlassSheetTokens.actionHPadding, 16)
        XCTAssertEqual(GlassSheetTokens.actionVPadding, 10)
    }

    private func channels(of color: Color) -> (red: Double, green: Double, blue: Double, alpha: Double) {
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        UIColor(color).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return (Double(red), Double(green), Double(blue), Double(alpha))
    }
}
