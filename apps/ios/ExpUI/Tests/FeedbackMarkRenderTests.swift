import ExpCore
import SwiftUI
import XCTest
@testable import ExpUI

// EXP-1259 D8.4: the mark's pixels. The loop's phase 0 is a plain disc, so a
// mark that does not move (Reduce Motion, `animated: false`, a capture, a
// timeline frozen on its first frame) must paint the static logo instead.
@MainActor
final class FeedbackMarkRenderTests: XCTestCase {
    private static let size: CGFloat = 100

    /// Pixels inside the disc that a cut cleared, rendered at 1x.
    private func clearedPixels(_ view: some View) throws -> Int {
        let renderer = ImageRenderer(content: view.frame(width: Self.size, height: Self.size))
        renderer.scale = 1
        let image = try XCTUnwrap(renderer.cgImage)
        let width = image.width
        let height = image.height
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        let context = try XCTUnwrap(CGContext(
            data: &bytes, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ))
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        var cleared = 0
        let center = Double(width) / 2
        // Inside a radius of 40 of the 50 disc: the antialiased rim never counts.
        for y in 0..<height {
            for x in 0..<width {
                let dx = Double(x) + 0.5 - center
                let dy = Double(y) + 0.5 - center
                guard dx * dx + dy * dy < 40 * 40 else { continue }
                if bytes[(y * width + x) * 4 + 3] < 128 { cleared += 1 }
            }
        }
        return cleared
    }

    func testPhaseZeroIsThePlainDisc() throws {
        XCTAssertEqual(try clearedPixels(MarkCanvas(color: .white, spans: FeedbackStates.Mark.spans(phase: 0))), 0)
    }

    func testANonAnimatingMarkIsTheStaticLogo() throws {
        let logo = try clearedPixels(MarkCanvas(color: .white, spans: FeedbackStates.Mark.staticSpans))
        // Three whole cuts, each ~6 wide across most of the disc.
        XCTAssertGreaterThan(logo, 900)
        XCTAssertEqual(try clearedPixels(FeedbackMark(size: Self.size, animated: false)), logo)
    }

    /// A snapshot of an animating mark is its first frame, which is the loop
    /// at rest: the static logo, never the plain disc.
    func testTheAnimatedMarksFirstFrameIsTheStaticLogo() throws {
        let logo = try clearedPixels(MarkCanvas(color: .white, spans: FeedbackStates.Mark.staticSpans))
        XCTAssertEqual(try clearedPixels(FeedbackMark(size: Self.size)), logo)
    }
}
