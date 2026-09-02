import Foundation
import SwiftUI
import UIKit
import XCTest
import ExpUI

// EXP-698: `GlassTokens` is the ONE place mobile reads the glass vocabulary,
// and every member of it must be a plain `DesignTokens` read — the generated
// mirror of packages/design-tokens/tokens.json that Android, web and the
// desktop generate from too. The moment a member carries its own literal, iOS
// has a second palette again (the hand-typed `Zinc` ramp this issue deleted),
// and the four clients drift a shade apart per release.
//
// The "no material, no shadow, no gradient palette" rules are STRUCTURAL —
// `GlassCard`/`GlassRow`/`GlassButton`/`GlassSection` simply have no
// `.ultraThinMaterial` or `.shadow` layer — so there is nothing here to
// assert; adding one would show up as a diff on the modifiers themselves.
final class GlassTokensTests: XCTestCase {

    // MARK: - Fills

    func testFillsAreTheSharedGlassTokens() {
        assertSameColor(GlassTokens.fillSection, DesignTokens.Glass.fillSection, "fillSection")
        assertSameColor(GlassTokens.fillRow, DesignTokens.Glass.fillRow, "fillRow")
        assertSameColor(GlassTokens.fillCard, DesignTokens.Glass.fillCard, "fillCard")
        assertSameColor(GlassTokens.fillActive, DesignTokens.Glass.fillActive, "fillActive")
    }

    // The ladder is ordered — a row is dimmer than a card, a card than active.
    // A reordering would silently invert the visual hierarchy.
    func testFillLadderIsOrdered() {
        let section = channels(of: GlassTokens.fillSection).alpha
        let row = channels(of: GlassTokens.fillRow).alpha
        let card = channels(of: GlassTokens.fillCard).alpha
        let active = channels(of: GlassTokens.fillActive).alpha
        XCTAssertLessThan(section, row)
        XCTAssertLessThan(row, card)
        XCTAssertLessThan(card, active)
    }

    // MARK: - Strokes

    func testStrokesAreTheSharedGlassTokens() {
        assertSameColor(GlassTokens.strokeRow, DesignTokens.Glass.strokeRow, "strokeRow")
        assertSameColor(GlassTokens.strokeSection, DesignTokens.Glass.strokeSection, "strokeSection")
        assertSameColor(GlassTokens.strokeCard, DesignTokens.Glass.strokeCard, "strokeCard")
        assertSameColor(GlassTokens.strokeStrong, DesignTokens.Glass.strokeStrong, "strokeStrong")
        assertSameColor(GlassTokens.strokeActive, DesignTokens.Glass.strokeActive, "strokeActive")
    }

    func testStrokeLadderIsOrdered() {
        let row = channels(of: GlassTokens.strokeRow).alpha
        let section = channels(of: GlassTokens.strokeSection).alpha
        let card = channels(of: GlassTokens.strokeCard).alpha
        let strong = channels(of: GlassTokens.strokeStrong).alpha
        let active = channels(of: GlassTokens.strokeActive).alpha
        XCTAssertLessThan(row, section)
        XCTAssertLessThan(section, card)
        XCTAssertLessThan(card, strong)
        XCTAssertLessThan(strong, active)
    }

    // Every glass fill and stroke is WHITE at an alpha — never a tinted gray.
    // A tinted one would sit differently on the two background rungs.
    func testEveryGlassValueIsWhiteAtAnAlpha() {
        let values: [(String, Color)] = [
            ("fillSection", GlassTokens.fillSection),
            ("fillRow", GlassTokens.fillRow),
            ("fillCard", GlassTokens.fillCard),
            ("fillActive", GlassTokens.fillActive),
            ("strokeRow", GlassTokens.strokeRow),
            ("strokeSection", GlassTokens.strokeSection),
            ("strokeCard", GlassTokens.strokeCard),
            ("strokeStrong", GlassTokens.strokeStrong),
            ("strokeActive", GlassTokens.strokeActive),
        ]
        for (name, color) in values {
            let c = channels(of: color)
            XCTAssertEqual(c.red, 1, accuracy: 0.0001, "\(name) red")
            XCTAssertEqual(c.green, 1, accuracy: 0.0001, "\(name) green")
            XCTAssertEqual(c.blue, 1, accuracy: 0.0001, "\(name) blue")
            XCTAssertLessThan(c.alpha, 1, "\(name) must be translucent")
        }
    }

    // MARK: - Geometry

    // One physical pixel on a 2x screen. Every glass stroke uses it; a 1pt
    // border reads as a heavy outline next to the rest of the app.
    func testHairlineIsHalfAPoint() {
        XCTAssertEqual(GlassTokens.hairline, 0.5)
    }

    func testRadiiAreTheSharedRungs() {
        XCTAssertEqual(GlassTokens.rowRadius, DesignTokens.Radius.md)
        XCTAssertEqual(GlassTokens.groupRadius, DesignTokens.Radius.lg)
        XCTAssertEqual(GlassTokens.cardRadius, DesignTokens.Radius.xl)
        XCTAssertEqual(GlassTokens.chipRadius, DesignTokens.Radius.sm)
        XCTAssertEqual(GlassTokens.fieldRadius, DesignTokens.Radius.lg)
        // The concrete numbers, so a tokens.json edit that changes what a rung
        // MEANS shows up here and not only in a screenshot diff.
        XCTAssertEqual(GlassTokens.chipRadius, 8)
        XCTAssertEqual(GlassTokens.rowRadius, 10)
        XCTAssertEqual(GlassTokens.groupRadius, 12)
        XCTAssertEqual(GlassTokens.cardRadius, 16)
    }

    // The ONE trailing-control size (EXP-698): every list-row and toolbar
    // circle is this, so the `…` on a comment, a device and a toolbar are the
    // same button.
    func testControlSizeIsTheSharedMediumControl() {
        XCTAssertEqual(GlassTokens.controlSize, DesignTokens.Size.controlMd)
        XCTAssertEqual(GlassTokens.controlSize, 32)
    }

    // MARK: - Background

    func testBackgroundPairIsTheSharedGlassTokens() {
        assertSameColor(GlassTokens.backgroundTop, DesignTokens.Glass.backgroundTop, "backgroundTop")
        assertSameColor(
            GlassTokens.backgroundBottom, DesignTokens.Glass.backgroundBottom, "backgroundBottom"
        )
    }

    // `opaqueCardFill` is DERIVED (fillCard over the card surface), never a
    // literal — chrome that floats over scrolling content must be opaque or
    // the feed shows through it.
    func testOpaqueCardFillIsTheOpaqueComposite() {
        let fill = channels(of: GlassTokens.opaqueCardFill)
        XCTAssertEqual(fill.alpha, 1, accuracy: 0.0001, "floating chrome must be opaque")

        let card = channels(of: DesignTokens.Palette.card)
        let tint = channels(of: DesignTokens.Glass.fillCard)
        let tolerance = 1.0 / 255.0
        for (name, base, top, got) in [
            ("red", card.red, tint.red, fill.red),
            ("green", card.green, tint.green, fill.green),
            ("blue", card.blue, tint.blue, fill.blue),
        ] {
            let expected = base * (1 - tint.alpha) + top * tint.alpha
            XCTAssertEqual(got, expected, accuracy: tolerance, "\(name) channel")
        }
    }

    // The composite must be BRIGHTER than the bare card surface — otherwise
    // the floating pill is indistinguishable from a card behind it.
    func testOpaqueCardFillIsBrighterThanTheCardSurface() {
        let fill = channels(of: GlassTokens.opaqueCardFill)
        let card = channels(of: DesignTokens.Palette.card)
        XCTAssertGreaterThan(fill.red, card.red)
    }

    // MARK: - Helpers

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
