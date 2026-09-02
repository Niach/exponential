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

    // The container is the flat `fillRow` surface with the brightest resting
    // hairline — the same pair Android's `GlassSegmentedControl` composes.
    func testContainerChromeIsTheSharedGlassTokens() {
        assertSameColor(
            GlassSegmentedControlTokens.containerFill, GlassTokens.fillRow, "containerFill"
        )
        assertSameColor(GlassSegmentedControlTokens.stroke, GlassTokens.strokeStrong, "stroke")
        XCTAssertEqual(GlassSegmentedControlTokens.hairline, GlassTokens.hairline)
        XCTAssertEqual(GlassSegmentedControlTokens.hairline, 0.5)
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

    // The three geometry numbers Android pins verbatim: the capsule-in-capsule
    // inset, the gap between segments, and the vertical padding that sets the
    // strip's height.
    func testGeometryMatchesTheAndroidDefaults() {
        XCTAssertEqual(GlassSegmentedControlTokens.capsulePadding, 4)
        XCTAssertEqual(GlassSegmentedControlTokens.segmentSpacing, 4)
        XCTAssertEqual(GlassSegmentedControlTokens.segmentVerticalPadding, 7)
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
