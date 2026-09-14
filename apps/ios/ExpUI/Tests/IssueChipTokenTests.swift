import Foundation
import SwiftUI
import UIKit
import XCTest
import ExpUI

// EXP-885: iOS used to draw an issue badge three ways — the markdown painter's
// bordered rounded rect, the composer's `GlassPill` capsule (identifier only,
// no title) and the session feed's full-width `glassRow`. `IssueChip` is the
// ONE badge outside the text painter, and the point of it is that it takes its
// PAINT from the markdown chip's own tokens: the two cannot drift apart
// because there is only one set of numbers.
//
// A SwiftUI view's rendering is not assertable in a unit test, so what is
// pinned here is that seam — the tokens the view reads — plus the geometry the
// view owns.
final class IssueChipTokenTests: XCTestCase {

    // MARK: - The shared seam

    // The glyph size is one constant for BOTH drawers: `MarkdownLayoutManager`
    // paints it over the chip's hidden `#` cell, `IssueChip` lays it out as a
    // view. Two constants would drift silently — a chip in prose and a chip in
    // the composer would wear different-sized status marks.
    func testTheStatusGlyphSizeIsShared() {
        XCTAssertEqual(MarkdownStyle.chipStatusIconSize, 13)
    }

    // The chip's inter-element gap IS the advance the painter kerns onto the
    // hidden `#`, which is what makes the glyph sit the same distance from the
    // identifier in prose and in a view.
    func testTheSpacingIsTheKernedGlyphGap() {
        XCTAssertEqual(IssueChipTokens.spacing, MarkdownStyle.chipStatusIconGap)
        XCTAssertEqual(IssueChipTokens.spacing, 6)
    }

    // A chip is a small rounded RECT with a hairline — never a capsule
    // (`GlassPill` is a different object with a different job).
    func testTheShapeIsTheMarkdownChipShape() {
        XCTAssertEqual(MarkdownStyle.chipCornerRadius, 5)
        XCTAssertEqual(IssueChipTokens.borderWidth, 1)
    }

    // The three colours are the markdown chip's, stated as UIColors so a
    // change to either side shows up here.
    func testThePaintIsTheMarkdownChipPaint() {
        XCTAssertEqual(MarkdownStyle.chipBackground, PlatformColor.white.withAlphaComponent(0.08))
        XCTAssertEqual(MarkdownStyle.chipBorder, PlatformColor.white.withAlphaComponent(0.16))
        XCTAssertEqual(MarkdownStyle.chipTokenColor, PlatformColor.white.withAlphaComponent(0.55))
    }

    // MARK: - Geometry the view owns

    func testPaddings() {
        XCTAssertEqual(IssueChipTokens.horizontalPadding, 6)
        XCTAssertEqual(IssueChipTokens.verticalPadding, 3)
    }

    // The ✕ is small (it is not the chip's subject) but its target is a real
    // one — the same 28pt clear button the composer's chip has always had.
    func testTheRemoveTargetIsThumbSized() {
        XCTAssertEqual(IssueChipTokens.removeGlyphSize, 10)
        XCTAssertEqual(IssueChipTokens.removeHitSize, 28)
    }

    // MARK: - Title

    // The chip cuts its title at the SAME 60 characters the markdown chip does
    // (web's `MAX_CHIP_TITLE_LENGTH`), so one issue reads identically wherever
    // it is chipped.
    func testTheTitleTakesTheSharedSixtyCharacterCut() {
        let long = String(repeating: "x", count: 80)
        XCTAssertEqual(IssueRefs.chipTitle(long), String(repeating: "x", count: 59) + "…")
        XCTAssertEqual(IssueRefs.chipTitle("  Fix login flow  "), "Fix login flow")
    }

    // ...and beyond the cut it still truncates: 60 characters of body text is
    // wider than a phone, and the composer's horizontal scroller bounds
    // nothing.
    func testTheTitleIsBounded() {
        XCTAssertEqual(IssueChipTokens.titleMaxWidth, 220)
    }

    // MARK: - Measure

    // The chip follows the prose it sits in (EXP-787's seam): `nil` is the
    // interchange body font, and the transcript's 14pt makes a chip that
    // matches the narration it sits under.
    func testTheChipFollowsTheProseMeasure() {
        XCTAssertEqual(
            MarkdownStyle.resolvedBodyFont(nil).pointSize, MarkdownStyle.bodyFont.pointSize)
        XCTAssertEqual(DesignTokens.Transcript.bodySize, 14)
        XCTAssertNotEqual(
            MarkdownStyle.resolvedBodyFont(DesignTokens.Transcript.bodySize).pointSize,
            MarkdownStyle.bodyFont.pointSize
        )
    }
}
