import Foundation
import SwiftUI
import UIKit
import XCTest
import ExpUI

// EXP-603: the glass menu is a cross-client contract, not a per-platform taste
// call. Android pins the same numbers in
// `app/src/test/java/com/exponential/app/ui/components/GlassMenuDefaultsTest.kt`
// — this is its mirror, so a drift in either platform's chrome breaks a build
// instead of shipping two different-looking menus.
//
// The "no shadow, no material blur" rule is STRUCTURAL — `GlassMenuSurface`
// simply has no `.shadow` / `.ultraThinMaterial` layer — so there is nothing
// here to assert; adding one would show up as a diff on this file's twin, the
// surface itself.
final class GlassMenuTokenTests: XCTestCase {

    // 12pt corners (Android's `SectionRadius`), not the 4dp M3 / 6pt system
    // default that reads as a different product.
    func testMenuGeometryMatchesTheAndroidDefaults() {
        XCTAssertEqual(GlassMenuTokens.radius, 12)
        XCTAssertEqual(GlassMenuTokens.hairline, 0.5)
        XCTAssertEqual(GlassMenuTokens.strokeOpacity, 0.10, accuracy: 0.0001)
    }

    // Material pins menu rows at 48dp with 12dp horizontal padding — already
    // past the 44pt touch target, so both platforms keep those exact metrics.
    func testMenuRowMetricsMatchTheAndroidDefaults() {
        XCTAssertEqual(GlassMenuTokens.itemMinHeight, 48)
        XCTAssertEqual(GlassMenuTokens.itemHPadding, 12)
    }

    // The same white .06 hairline every other divider in the app uses.
    func testDividerIsTheSharedRowHairline() {
        XCTAssertEqual(GlassMenuTokens.dividerOpacity, 0.06, accuracy: 0.0001)
    }

    // The fill is DERIVED (white .06 over the card token), never a literal —
    // but it has to land on the identical opaque #252525 Android composites
    // as `GlassTokens.OpaqueCardFill`, or a menu reads a different gray on
    // each phone.
    func testOpaqueFillCompositesToTheSharedMenuGray() {
        let card = channels(of: DesignTokens.Palette.card)
        let tint = GlassMenuTokens.tintOpacity
        let expected = 37.0 / 255.0
        let tolerance = 1.0 / 255.0

        for (name, base) in [("red", card.red), ("green", card.green), ("blue", card.blue)] {
            let composited = base * (1 - tint) + 1 * tint
            XCTAssertEqual(composited, expected, accuracy: tolerance, "\(name) channel")
        }
        XCTAssertEqual(card.alpha, 1, accuracy: 0.0001, "the base must be OPAQUE — a menu floats over content")
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
