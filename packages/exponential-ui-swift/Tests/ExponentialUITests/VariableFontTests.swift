import Foundation
import CoreText
import XCTest
@testable import ExponentialUI

#if canImport(AppKit) && !canImport(UIKit)
import AppKit
#endif

/// VAPP-166 (the spezl drafts on eUI 0.2.0): CallSurface painted a 24 px
/// "spezl" as "spe…" with Nunito / Fraunces VARIABLE files registered.
/// Two causes, locked here with `conformance/fonts/Nunito-Variable.ttf`
/// (Google Fonts' Nunito[wght], every named instance OS/2 200):
/// - every named instance read as weight 200, so a title at 600 painted an
///   arbitrary instance; the weight is now the instance's `wght`, and a
///   variable face paints the requested weight itself (CSS);
/// - a label in a box exactly its measured width (pixel-snapped) let
///   TextKit's own fit test truncate it; a fitting line now never does.
final class VariableFontTests: XCTestCase {
    static var fonts: URL {
        Fixtures.dir.deletingLastPathComponent().appendingPathComponent("conformance/fonts")
    }

    /// The faces of `files` WITHOUT registering them with the process (the
    /// named instances share PostScript names with the static Nunito the
    /// conformance suite registers).
    static func faces(_ files: [String]) -> [TextFonts.Face] {
        files.flatMap { file -> [TextFonts.Face] in
            let url = fonts.appendingPathComponent(file) as CFURL
            return ((CTFontManagerCreateFontDescriptorsFromURL(url) as? [CTFontDescriptor]) ?? []).map(TextFonts.face(of:))
        }
    }

    override func setUp() async throws {
        try XCTSkipUnless(FileManager.default.fileExists(atPath: Self.fonts.appendingPathComponent("Nunito-Variable.ttf").path), "conformance fonts not in this checkout")
        TextFonts.install(TextFonts.Set(faces: [
            "NunitoVariable": Self.faces(["Nunito-Variable.ttf"]),
            "NunitoStatic": Self.faces(["Nunito-Regular.ttf", "Nunito-SemiBold.ttf", "Nunito-ExtraBold.ttf"]),
        ]))
    }

    override func tearDown() async throws {
        TextFonts.install(nil)
    }

    private func title(_ family: String, _ weight: Int) -> TextStyle {
        TextStyle(fontSize: 24, fontWeight: weight, lineHeight: 32, fontFamily: family)
    }

    func testNamedInstancesCarryTheirWghtNotTheSharedUsWeightClass() {
        let weights = Self.faces(["Nunito-Variable.ttf"]).map(\.weight).sorted()
        XCTAssertEqual(weights, [200, 300, 400, 500, 600, 700, 800, 900])
        XCTAssertTrue(Self.faces(["Nunito-Variable.ttf"]).allSatisfy { $0.weights == 200...1000 })
        XCTAssertTrue(Self.faces(["Nunito-SemiBold.ttf"]).allSatisfy { $0.weight == 600 && $0.weights == nil })
    }

    func testVariableWeightsPaintLikeTheStaticFaces() {
        let text = "spezl"
        let light = TextShaper.width(text, title("NunitoVariable", 200))
        for weight in [400, 600, 800] {
            let variable = TextShaper.width(text, title("NunitoVariable", weight))
            let fixed = TextShaper.width(text, title("NunitoStatic", weight))
            XCTAssertGreaterThan(variable, light + 1, "\(weight) paints heavier than 200")
            XCTAssertEqual(variable, fixed, accuracy: fixed * 0.01, "\(weight) = the static \(weight) face")
        }
        // Between named instances: the requested weight itself (CSS), not
        // the nearest instance.
        let w650 = TextShaper.width(text, title("NunitoVariable", 650))
        XCTAssertGreaterThan(w650, TextShaper.width(text, title("NunitoVariable", 600)))
        XCTAssertLessThan(w650, TextShaper.width(text, title("NunitoVariable", 700)))
    }

    func testMeasuredWidthIsThePaintedWidth() {
        for weight in [200, 600, 650, 800] {
            let ts = title("NunitoVariable", weight)
            let painted = TextShaper.attributes(ts)[.font] as! PlatformFont
            XCTAssertEqual(CTFontCopyPostScriptName(painted as CTFont) as String, CTFontCopyPostScriptName(TextFonts.font(family: ts.fontFamily, weight: weight, size: 24)) as String)
            XCTAssertEqual(TextShaper.lineWidth(TextShaper.attributed("spezl", ts)), TextShaper.width("spezl", ts), accuracy: 0.02, "\(weight)")
        }
    }

    func testAFittingLineNeverTruncates() {
        let ts = title("NunitoVariable", 600)
        let text = TextShaper.attributed("spezl", ts, align: .center, lineBreak: .byTruncatingTail)
        let measured = TextShaper.width("spezl", ts)
        for scale: CGFloat in [1, 2, 3] {
            // The box snapped DOWN a pixel's worth still fits.
            let snapped = ((measured * scale).rounded(.down) - 1) / scale
            XCTAssertTrue(TextLabel.fitsOneLine(text, width: snapped, scale: scale), "@\(scale)x")
            XCTAssertFalse(TextLabel.fitsOneLine(text, width: measured / 2, scale: scale), "@\(scale)x")
        }
        XCTAssertFalse(TextLabel.fitsOneLine(TextShaper.attributed("spe\nzl", ts), width: 1000, scale: 2), "a hard break is never one line")
        let clipped = TextLabel.breaking(text, .byClipping)
        XCTAssertEqual((clipped.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.lineBreakMode, .byClipping)
    }

    #if canImport(AppKit) && !canImport(UIKit)
    /// The draft's case: a 1-line title in a box its measured width
    /// (+ the field's alignment insets, snapped to 2x) drops the ellipsis;
    /// a box half as wide keeps it.
    @MainActor
    func testAppKitLabelDropsTheEllipsisWhenTheLineFits() {
        let ts = title("NunitoVariable", 600)
        let measured = TextShaper.width("spezl", ts)
        let label = TextLabel.Label(labelWithAttributedString: NSAttributedString())
        label.source = TextShaper.attributed("spezl", ts, align: .center, lineBreak: .byTruncatingTail)
        label.wantedLines = 1
        label.wantedBreak = .byTruncatingTail
        let insets = label.alignmentRectInsets.left + label.alignmentRectInsets.right
        label.setFrameSize(NSSize(width: ((measured + insets) * 2).rounded(.down) / 2, height: 32))
        XCTAssertEqual(label.lineBreakMode, .byClipping)
        XCTAssertEqual((label.attributedStringValue.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.lineBreakMode, .byClipping)
        XCTAssertEqual(label.attributedStringValue.string, "spezl")
        label.setFrameSize(NSSize(width: measured / 2, height: 32))
        XCTAssertEqual(label.lineBreakMode, .byTruncatingTail)
        XCTAssertEqual((label.attributedStringValue.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.lineBreakMode, .byTruncatingTail)
    }
    #endif
}
