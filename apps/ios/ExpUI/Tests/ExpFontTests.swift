import SwiftUI
import UIKit
import XCTest
@testable import ExpUI

/// #EXP-2215: every text style is Inter on the shared token scale (web's
/// text-xs…2xl, Android's pairs), from the SDK's bundled faces.
final class ExpFontTests: XCTestCase {
    func testTextStylesMapOntoTheTokenScale() {
        typealias S = DesignTokens.Typography.Size
        let expected: [(Font.TextStyle, CGFloat)] = [
            (.largeTitle, S.xl2), (.title, S.xl), (.title2, S.xl), (.title3, S.xl),
            (.headline, S.base), (.body, S.base), (.callout, S.base),
            (.subheadline, S.sm), (.footnote, S.xs), (.caption, S.xs), (.caption2, S.xs),
        ]
        for (style, size) in expected { XCTAssertEqual(ExpType.size(style), size, "\(style)") }
    }

    func testUIKitFontsAreTheBundledInterFaces() {
        let large = UITraitCollection(preferredContentSizeCategory: .large)
        let body = UIFont.exp(.body)
        XCTAssertEqual(body.familyName, "Inter")
        XCTAssertEqual(UIFont.exp(.subheadline, weight: .semibold).fontName, "Inter-SemiBold")
        XCTAssertEqual(UIFont.inter(size: 13, weight: .medium).fontName, "Inter-Medium")
        XCTAssertEqual(UIFontMetrics(forTextStyle: .body).scaledFont(for: UIFont.inter(size: 16), compatibleWith: large).pointSize, 16)
    }

    func testMarkdownMarksSurviveOnInter() {
        let body = MarkdownStyle.bodyFont
        XCTAssertEqual(body.familyName, "Inter")
        let bold = expBoldFont(body)
        XCTAssertTrue(expFontHasBold(bold))
        XCTAssertEqual(bold.familyName, "Inter")
        // Inter ships no italic: the run reads in the system italic, its
        // trait (the serializer's `*`) kept.
        XCTAssertTrue(expFontHasItalic(expItalicFont(body)))
        XCTAssertTrue(expFontHasItalic(expItalicFont(bold)) && expFontHasBold(expItalicFont(bold)))
    }
}
