import XCTest
import SwiftUI
@testable import ExponentialUI

#if canImport(AppKit)
import AppKit
#endif

/// VAPP-100: a single line drawn in a box exactly its MEASURED width must
/// never truncate. The view system snaps the box to the pixel grid and
/// TextKit ceils the text's width to a pixel, so the label lays out in
/// `TextLabel.paintRect` (bounds + 2 px towards the text's growth side).
/// iOS showed "Accou…" for the Stepper's "Account" (54.781 → 54.667 at 3x,
/// TextKit needs 55.000).
final class TextLabelPaintRectTests: XCTestCase {
    /// `v` ceiled / floored to a device pixel.
    private func ceilPx(_ v: CGFloat, _ scale: CGFloat) -> CGFloat { (v * scale - 0.0001).rounded(.up) / scale }
    private func floorPx(_ v: CGFloat, _ scale: CGFloat) -> CGFloat { (v * scale + 0.0001).rounded(.down) / scale }

    func testMeasuredLineFitsAfterPixelSnapping() {
        let styles = [
            TextStyle(fontSize: 14, fontWeight: 500, lineHeight: 20, fontFamily: "Inter"),
            TextStyle(fontSize: 12, fontWeight: 400, lineHeight: 16, fontFamily: nil),
            TextStyle(fontSize: 13, fontWeight: 600, lineHeight: 18, fontFamily: "ui-monospace"),
        ]
        let texts = ["Account", "ios", "2", "size · sm", "Delete board?", "WWWWWWW"]
        for ts in styles {
            for text in texts {
                let measured = TextShaper.width(text, ts)
                for scale: CGFloat in [1, 2, 3] {
                    // Worst case: the frame snapped DOWN a pixel's worth.
                    let snapped = CGRect(x: 0, y: 0, width: floorPx(measured, scale), height: ts.lineHeight)
                    let needed = ceilPx(measured, scale)
                    for alignment: NSTextAlignment in [.left, .center, .right, .natural] {
                        let box = TextLabel.paintRect(snapped, alignment: alignment, rtl: false, scale: scale)
                        XCTAssertGreaterThanOrEqual(box.width + 0.0001, needed, "\(text) @\(scale)x \(alignment.rawValue)")
                    }
                }
            }
        }
    }

    func testSlackGrowsTowardsTheAlignment() {
        let r = CGRect(x: 10, y: 0, width: 50, height: 20)
        let left = TextLabel.paintRect(r, alignment: .left, rtl: false, scale: 2)
        XCTAssertEqual(left.minX, 10)
        XCTAssertEqual(left.width, 51)
        let right = TextLabel.paintRect(r, alignment: .right, rtl: false, scale: 2)
        XCTAssertEqual(right.maxX, 60)
        XCTAssertEqual(right.width, 51)
        let center = TextLabel.paintRect(r, alignment: .center, rtl: false, scale: 2)
        XCTAssertEqual(center.midX, r.midX)
        // Natural under rtl starts at the right edge.
        let natural = TextLabel.paintRect(r, alignment: .natural, rtl: true, scale: 3)
        XCTAssertEqual(natural.maxX, 60, accuracy: 0.0001)
        XCTAssertEqual(natural.width, 50 + 2.0 / 3, accuracy: 0.0001)
    }

    #if canImport(AppKit) && !canImport(UIKit)
    /// The AppKit label draws through the widening cell.
    @MainActor
    func testAppKitLabelUsesTheWideningCell() {
        let label = TextLabel.Label(labelWithAttributedString: NSAttributedString(string: "Account"))
        XCTAssertTrue(label.cell is TextLabel.Cell)
        let bounds = NSRect(x: 0, y: 0, width: 54.5, height: 20)
        let drawn = label.cell!.drawingRect(forBounds: bounds)
        let plain = NSTextFieldCell(textCell: "Account").drawingRect(forBounds: bounds)
        XCTAssertGreaterThan(drawn.width, plain.width)
    }
    #endif
}
