import Foundation
import SwiftUI
import XCTest
import ExpUI

// EXP-698 r5: `GlassMetaRow` is the ONE property row a `.glassSection()` group
// stacks — the issue Properties sheet's rows. Before the lift the sheet drew
// its own near-copy (a leading gutter glyph, a trailing chevron, a 44pt floor,
// 14pt gutters), so the same five properties looked like two different lists
// depending on whether the issue existed yet. (EXP-1170: the New issue page
// wears the issue face's chip box instead.)
final class GlassMetaRowTokenTests: XCTestCase {

    func testTheRowPaddingsAreTheAndroidMetaRowNumbers() {
        XCTAssertEqual(GlassMetaRowTokens.horizontalPadding, 16)
        XCTAssertEqual(GlassMetaRowTokens.verticalPadding, 12)
    }

    // The glyph leading the VALUE reads with the `.subheadline` beside it, not
    // on its own — smaller than the `.body` rung, and smaller than the 16pt
    // `AppIcon.Size.medium` a row's own leading glyph would take.
    func testTheValueGlyphIsSmallerThanAControlGlyph() {
        XCTAssertEqual(GlassMetaRowTokens.glyphSize, 14)
        XCTAssertLessThan(GlassMetaRowTokens.glyphSize, AppIcon.Size.medium)
    }

    // It builds: the row is a plain View with no environment requirements, so
    // a sheet, a scroll column and a preview host can all stack it.
    func testTheRowBuilds() {
        let row = GlassMetaRow(
            label: "Status",
            icon: AppIcons.statusInProgress,
            iconColor: .white,
            value: "In Progress"
        ) {}
        XCTAssertNotNil(UIHostingController(rootView: row).view)
    }
}
