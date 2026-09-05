import CoreGraphics
import XCTest
import ExpUI

// EXP-744: where a glass menu lands, as pure geometry. The `…` on the steering
// screen hangs off a navigation bar taller than its glyph's capsule (the
// two-line title), so "8pt under the glyph" put the menu's head under the
// bar's material. The popup now clamps to the host's top edge; these pin
// the clamp and the pre-existing rules around it.
final class GlassMenuPlacementTests: XCTestCase {

    private let screen = CGSize(width: 402, height: 874)
    private let menuSize = CGSize(width: 200, height: 96)

    private func place(anchor: CGRect, top: CGFloat = 0) -> GlassMenuPlacement {
        GlassMenuPlacement(anchor: anchor, top: top, screen: screen, menuSize: menuSize)
    }

    // The steering screen (EXP-744): a 32pt glyph whose bottom sits above the
    // bar's bottom edge. The menu opens under the BAR, not under the glyph.
    func testMenuNeverOpensUnderTheBarItHangsFrom() {
        let glyph = CGRect(x: 350, y: 70, width: 32, height: 32)
        let barBottom: CGFloat = 116
        let placement = place(anchor: glyph, top: barBottom)
        XCTAssertTrue(placement.opensBelow)
        XCTAssertEqual(placement.origin.y, barBottom + GlassMenuTokens.anchorGap)
    }

    // A bar exactly as tall as the glyph's capsule (issue detail, support):
    // the clamp is inert and the menu still hangs off the glyph.
    func testAnchorBelowTheTopEdgeIsLeftAlone() {
        let glyph = CGRect(x: 350, y: 70, width: 32, height: 32)
        let placement = place(anchor: glyph, top: 60)
        XCTAssertEqual(placement.origin.y, glyph.maxY + GlassMenuTokens.anchorGap)
    }

    // Trailing-aligned to the anchor, inside the screen margin.
    func testTrailingAlignedAndClampedToTheScreen() {
        let glyph = CGRect(x: 350, y: 70, width: 32, height: 32)
        let placement = place(anchor: glyph)
        let maxX = screen.width - menuSize.width - GlassMenuTokens.screenMargin
        XCTAssertEqual(placement.origin.x, min(glyph.maxX - menuSize.width, maxX))

        let edge = place(anchor: CGRect(x: 380, y: 70, width: 32, height: 32))
        XCTAssertEqual(edge.origin.x, maxX)
    }

    // A row menu near the bottom flips above its anchor, and even then never
    // crosses the host's top edge.
    func testFlipsAboveWhenItWouldRunOffTheBottom() {
        let row = CGRect(x: 350, y: 820, width: 32, height: 32)
        let placement = place(anchor: row, top: 60)
        XCTAssertFalse(placement.opensBelow)
        XCTAssertEqual(placement.origin.y, row.minY - GlassMenuTokens.anchorGap - menuSize.height)

        let tallMenu = GlassMenuPlacement(
            anchor: row, top: 60, screen: screen, menuSize: CGSize(width: 200, height: 800)
        )
        XCTAssertFalse(tallMenu.opensBelow)
        XCTAssertEqual(tallMenu.origin.y, 60 + GlassMenuTokens.screenMargin)
    }

    // Before the menu has measured itself its size is zero; the below branch
    // is chosen and the clamp already applies, so the first real frame does
    // not jump.
    func testUnmeasuredMenuStillClampsToTheTopEdge() {
        let glyph = CGRect(x: 350, y: 70, width: 32, height: 32)
        let placement = GlassMenuPlacement(anchor: glyph, top: 116, screen: screen, menuSize: .zero)
        XCTAssertTrue(placement.opensBelow)
        XCTAssertEqual(placement.origin.y, 116 + GlassMenuTokens.anchorGap)
    }
}
