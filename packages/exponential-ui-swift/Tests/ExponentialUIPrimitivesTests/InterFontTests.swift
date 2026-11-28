import CoreText
import XCTest
@testable import ExponentialUIPrimitives

/// #EXP-2215: the bundled Inter faces register and resolve by weight.
final class InterFontTests: XCTestCase {
    func testTheFourFacesAreBundledAndRegister() {
        XCTAssertEqual(InterFont.urls.count, 4)
        XCTAssertTrue(InterFont.register())
        for (weight, name) in [(400, "Inter-Regular"), (500, "Inter-Medium"), (600, "Inter-SemiBold"), (700, "Inter-Bold")] {
            XCTAssertEqual(InterFont.postScriptName(weight: weight), name)
            let font = CTFontCreateWithName(name as CFString, 14, nil)
            XCTAssertEqual(CTFontCopyPostScriptName(font) as String, name)
            XCTAssertEqual(CTFontCopyFamilyName(font) as String, InterFont.family)
        }
    }

    func testPlatformInterPicksTheWeightsFace() {
        XCTAssertEqual(PlatformInter.font(size: 14).fontName, "Inter-Regular")
        XCTAssertEqual(PlatformInter.font(size: 14, weight: .medium).fontName, "Inter-Medium")
        XCTAssertEqual(PlatformInter.font(size: 14, weight: .semibold).fontName, "Inter-SemiBold")
        XCTAssertEqual(PlatformInter.font(size: 14, weight: .bold).fontName, "Inter-Bold")
    }
}
