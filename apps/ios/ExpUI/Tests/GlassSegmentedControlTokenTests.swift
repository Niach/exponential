import Foundation
import SwiftUI
import UIKit
import XCTest
import ExpUI

// EXP-698: the segmented strip is a cross-client contract, not a per-platform
// taste call. Android pins the same numbers in
// `app/src/test/java/com/exponential/app/ui/components/GlassSegmentedControlDefaultsTest.kt`
// — this is its mirror, so a drift in either platform's chrome breaks a build
// instead of shipping two different-looking strips.
//
// The CAPSULE shape and the "no material backdrop" rule are STRUCTURAL — the
// control clips to `Capsule()` and simply has no `.ultraThinMaterial` layer,
// so there is no radius number to pin and nothing here to assert; either would
// show up as a diff on the control itself.
final class GlassSegmentedControlTokenTests: XCTestCase {

    // The container is the DIMMEST rung with the stroke that pairs with it —
    // the same pair Android's `GlassSegmentedControl` composes. Dim on
    // purpose: the selected segment's `fillActive` has to be the brightest
    // thing in the strip, and a `fillRow` container ate most of that contrast.
    func testContainerChromeIsTheSharedGlassTokens() {
        assertSameColor(
            GlassSegmentedControlTokens.containerFill, GlassTokens.fillSection, "containerFill"
        )
        assertSameColor(GlassSegmentedControlTokens.stroke, GlassTokens.strokeSection, "stroke")
        XCTAssertEqual(GlassSegmentedControlTokens.hairline, GlassTokens.hairline)
        XCTAssertEqual(GlassSegmentedControlTokens.hairline, 0.5)
    }

    // The container must stay dimmer than the segment it holds — the whole
    // point of the selected fill is that it reads as raised off the strip.
    func testActiveSegmentIsBrighterThanItsContainer() {
        XCTAssertLessThan(
            alpha(of: GlassSegmentedControlTokens.containerFill),
            alpha(of: GlassSegmentedControlTokens.activeFill)
        )
    }

    // The selected segment wears the ONE bright glass fill — the same value a
    // selected row or an active pill wears, so "selected" reads identically
    // everywhere.
    func testActiveSegmentFillIsTheSharedActiveFill() {
        assertSameColor(GlassSegmentedControlTokens.activeFill, GlassTokens.fillActive, "activeFill")
        assertSameColor(
            GlassSegmentedControlTokens.activeFill, DesignTokens.Glass.fillActive, "activeFill token"
        )
    }

    // The geometry numbers Android pins verbatim: the capsule-in-capsule
    // inset, the gap between segments (none — they tile), the segment's own
    // vertical padding, and the standalone strip's fixed height.
    func testGeometryMatchesTheAndroidDefaults() {
        XCTAssertEqual(GlassSegmentedControlTokens.capsulePadding, 3)
        XCTAssertEqual(GlassSegmentedControlTokens.segmentSpacing, 0)
        XCTAssertEqual(GlassSegmentedControlTokens.segmentVerticalPadding, 6)
        XCTAssertEqual(GlassSegmentedControlTokens.height, DesignTokens.Size.controlLg)
        XCTAssertEqual(GlassSegmentedControlTokens.height, 36)
    }

    private func alpha(of color: Color) -> Double {
        channels(of: color).alpha
    }

    private func assertSameColor(
        _ got: Color,
        _ expected: Color,
        _ name: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        let a = channels(of: got)
        let b = channels(of: expected)
        XCTAssertEqual(a.red, b.red, accuracy: 0.0001, "\(name) red", file: file, line: line)
        XCTAssertEqual(a.green, b.green, accuracy: 0.0001, "\(name) green", file: file, line: line)
        XCTAssertEqual(a.blue, b.blue, accuracy: 0.0001, "\(name) blue", file: file, line: line)
        XCTAssertEqual(a.alpha, b.alpha, accuracy: 0.0001, "\(name) alpha", file: file, line: line)
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
