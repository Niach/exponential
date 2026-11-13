import Foundation
import SwiftUI
import UIKit
import XCTest
import ExpUI

// EXP-698 round 2: `GlassPill` is the ONE pill, and its two size rungs are the
// only heights a pill may have. Before it, iOS drew capsules at five different
// vertical paddings (3, 4, 5, 6, 10) with three different fonts, so a filter, a
// property chip and a repo chip in the same row were three different objects.
//
// These numbers are load-bearing beyond taste: `.sm` is `controlSm` and `.md`
// is `controlMd`, the same rungs the circle buttons and inputs sit on, which is
// what makes a pill line up with the control beside it. A quiet drift here is
// invisible in a diff and obvious on a phone.
//
// The chrome rules are STRUCTURAL — a pill is `.glassButton()`, i.e. `fillCard`
// + `strokeCard`, `fillActive` + `strokeActive` when a `.select` is selected —
// so there is nothing to assert here that `GlassTokensTests` does not already
// pin on those tokens; a change would show up as a diff on the pill itself.
final class GlassPillTokenTests: XCTestCase {

    // MARK: - Heights

    // The two rungs ARE the shared control sizes — not two numbers that happen
    // to match them today.
    func testHeightsAreTheSharedControlRungs() {
        XCTAssertEqual(GlassPillTokens.heightSm, DesignTokens.Size.controlSm)
        XCTAssertEqual(GlassPillTokens.heightMd, DesignTokens.Size.controlMd)
        XCTAssertEqual(GlassPillTokens.heightSm, 24)
        XCTAssertEqual(GlassPillTokens.heightMd, 32)
    }

    // `.md` matches `CircleIconButton`, so a pill and a circle button sharing a
    // row are the same height — the reason `.md` exists at all.
    func testMediumPillMatchesTheTrailingControlSize() {
        XCTAssertEqual(GlassPillTokens.heightMd, GlassTokens.controlSize)
    }

    // MARK: - Paddings

    func testHorizontalPaddings() {
        XCTAssertEqual(GlassPillTokens.horizontalPaddingSm, 8)
        XCTAssertEqual(GlassPillTokens.horizontalPaddingMd, 12)
    }

    func testSpacings() {
        XCTAssertEqual(GlassPillTokens.spacingSm, 4)
        XCTAssertEqual(GlassPillTokens.spacingMd, 6)
    }

    // EXP-1822: the pill numbers ARE the shared `tokens.json` sizes the web
    // `Pill`, Android `GlassPillDefaults` and the IDE read.
    func testEveryRungReadsTheSharedTokens() {
        XCTAssertEqual(GlassPillTokens.horizontalPaddingSm, DesignTokens.Size.pillPaddingSm)
        XCTAssertEqual(GlassPillTokens.horizontalPaddingMd, DesignTokens.Size.pillPaddingMd)
        XCTAssertEqual(GlassPillTokens.spacingSm, DesignTokens.Size.pillGapSm)
        XCTAssertEqual(GlassPillTokens.spacingMd, DesignTokens.Size.pillGapMd)
        XCTAssertEqual(GlassPillTokens.glyphSm, DesignTokens.Size.pillGlyphSm)
        XCTAssertEqual(GlassPillTokens.glyphMd, DesignTokens.Size.pillGlyphMd)
        XCTAssertEqual(GlassPillTokens.dotSize, DesignTokens.Size.pillDot)
    }

    // EXP-1822: the property chip box's metrics ×4: a 16pt face clearing the
    // 24pt capsule by 4, 8pt label dots, a 6pt wrap gap inside 10pt padding.
    func testPropertyChipMetrics() {
        XCTAssertEqual(GlassPillTokens.avatarSm, 16)
        XCTAssertEqual((GlassPillTokens.heightSm - GlassPillTokens.avatarSm) / 2, 4)
        XCTAssertEqual(GlassPillTokens.labelDot, 8)
        XCTAssertEqual(GlassPillTokens.chipBoxGap, 6)
        XCTAssertEqual(GlassPillTokens.chipBoxPadding, 10)
    }

    // MARK: - Glyphs

    func testGlyphSizes() {
        XCTAssertEqual(GlassPillTokens.glyphSm, 12)
        XCTAssertEqual(GlassPillTokens.glyphMd, 16)
    }

    // A call site building its OWN leading view (a spinner, a brand image, a
    // board icon) reads the size off the rung, so it lands on the same glyph
    // box the `icon:` init draws.
    func testSizeRungsExposeTheirGlyphSize() {
        XCTAssertEqual(GlassPillSize.sm.glyphSize, GlassPillTokens.glyphSm)
        XCTAssertEqual(GlassPillSize.md.glyphSize, GlassPillTokens.glyphMd)
    }

    // MARK: - Ladders

    // The rungs are ORDERED on every axis: a bigger pill is taller, roomier,
    // more widely spaced and carries a bigger glyph. An inversion on any one
    // of them would make `.md` read as the smaller control.
    func testRungsAreOrdered() {
        XCTAssertLessThan(GlassPillTokens.heightSm, GlassPillTokens.heightMd)
        XCTAssertLessThan(
            GlassPillTokens.horizontalPaddingSm, GlassPillTokens.horizontalPaddingMd
        )
        XCTAssertLessThan(GlassPillTokens.spacingSm, GlassPillTokens.spacingMd)
        XCTAssertLessThan(GlassPillTokens.glyphSm, GlassPillTokens.glyphMd)
    }

    // The glyph must fit inside the pill with padding to spare — a glyph as
    // tall as the capsule has no capsule left around it.
    func testGlyphsFitInsideTheirRung() {
        XCTAssertLessThan(GlassPillTokens.glyphSm, GlassPillTokens.heightSm)
        XCTAssertLessThan(GlassPillTokens.glyphMd, GlassPillTokens.heightMd)
    }

    // MARK: - Dot

    // The label/status dot is a small filled disc, not a second glyph: it has
    // to stay clearly under the glyph box or it reads as a bullet icon.
    func testDotIsSmallerThanTheSmallestGlyph() {
        XCTAssertEqual(GlassPillTokens.dotSize, 6)
        XCTAssertLessThan(GlassPillTokens.dotSize, GlassPillTokens.glyphSm)
    }
}
