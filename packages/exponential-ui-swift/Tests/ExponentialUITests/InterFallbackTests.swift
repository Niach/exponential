import CoreText
import XCTest
@testable import ExponentialUI

/// #EXP-2215: the theme's `Inter`, never registered by the host, resolves to
/// the faces `ExponentialUIPrimitives` bundles (it was the system font).
final class InterFallbackTests: XCTestCase {
    func testInterResolvesToTheBundledFacesByCssWeight() {
        TextFonts.install(nil)
        for (weight, name) in [(400, "Inter-Regular"), (500, "Inter-Medium"), (600, "Inter-SemiBold"), (700, "Inter-Bold"), (800, "Inter-Bold")] {
            let font = TextFonts.font(family: "Inter", weight: weight, size: 14)
            XCTAssertEqual(CTFontCopyPostScriptName(font) as String, name, "weight \(weight)")
        }
        XCTAssertEqual(TextFonts.resolve("Inter, system-ui").family, "Inter")
    }
}
