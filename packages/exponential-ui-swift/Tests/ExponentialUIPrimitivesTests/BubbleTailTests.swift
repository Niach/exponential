import SwiftUI
import XCTest
@testable import ExponentialUIPrimitives

/// #EXP-1876: the speech-bubble tail (style.json `tail`) as ONE outline
/// with its box: the tips, the side points, the square corner, the mirrors.
final class BubbleTailTests: XCTestCase {
    private let rect = CGRect(x: 0, y: 0, width: 200, height: 60)

    private func shape(_ tail: BubbleTail?, radius: CGFloat = 16) -> Path {
        TailedBoxShape(radius: radius, tail: tail).path(in: rect)
    }

    private func close(_ a: CGFloat, _ b: CGFloat, _ eps: CGFloat = 0.01) -> Bool { abs(a - b) < eps }

    func testResolveFollowsTheDirection() {
        XCTAssertEqual(BubbleTail.resolve("start", rtl: false), .left)
        XCTAssertEqual(BubbleTail.resolve("start", rtl: true), .right)
        XCTAssertEqual(BubbleTail.resolve("end", rtl: false), .right)
        XCTAssertEqual(BubbleTail.resolve("end", rtl: true), .left)
        XCTAssertEqual(BubbleTail.resolve("down", rtl: false), .downRight)
        XCTAssertEqual(BubbleTail.resolve("down", rtl: true), .downLeft)
        XCTAssertNil(BubbleTail.resolve("none", rtl: false))
        XCTAssertNil(BubbleTail.resolve(nil, rtl: false))
    }

    func testNoTailIsTheRoundedBox() {
        let b = shape(nil).boundingRect
        XCTAssertEqual(b, rect)
    }

    func testStartReachesSixOutOnTheTopEdge() {
        let p = shape(.left)
        let b = p.boundingRect
        // The rounded tip stops short of the sharp point (-6, 0).
        XCTAssertTrue(b.minX < -4.5 && b.minX > -6, "\(b)")
        XCTAssertTrue(close(b.minY, 0), "\(b)")
        XCTAssertTrue(close(b.maxX, 200) && close(b.maxY, 60), "\(b)")
        // Inside the tail, outside its hypotenuse, square corner.
        XCTAssertTrue(p.contains(CGPoint(x: -3, y: 1)))
        XCTAssertFalse(p.contains(CGPoint(x: -1, y: 8)))
        XCTAssertTrue(p.contains(CGPoint(x: 0.5, y: 0.5)))
        // The other corners keep their radius.
        XCTAssertFalse(p.contains(CGPoint(x: 199.5, y: 0.5)))
        XCTAssertFalse(p.contains(CGPoint(x: 0.5, y: 59.5)))
    }

    func testStartTipRoundsFromTheSpecPoints() {
        var points: [CGPoint] = []
        shape(.left).forEach { el in
            switch el {
            case let .move(to: p), let .line(to: p): points.append(p)
            case let .quadCurve(to: p, control: c): points.append(c); points.append(p)
            default: break
            }
        }
        XCTAssertTrue(points.contains { close($0.x, -4) && close($0.y, 0) })
        XCTAssertTrue(points.contains { close($0.x, 0) && close($0.y, 9) })
        XCTAssertTrue(points.contains { close($0.x, -4.891) && close($0.y, 1.664) })
        XCTAssertTrue(points.contains { close($0.x, -6) && close($0.y, 0) })
    }

    func testEndMirrorsStart() {
        let p = shape(.right)
        let b = p.boundingRect
        XCTAssertTrue(b.maxX > 204.5 && b.maxX < 206, "\(b)")
        XCTAssertTrue(close(b.minX, 0), "\(b)")
        XCTAssertTrue(p.contains(CGPoint(x: 203, y: 1)))
        XCTAssertTrue(p.contains(CGPoint(x: 199.5, y: 0.5)))
        XCTAssertFalse(p.contains(CGPoint(x: 0.5, y: 0.5)))
    }

    func testDownHangsNineBelow22In() {
        let p = shape(.downRight)
        let b = p.boundingRect
        XCTAssertTrue(b.maxY > 67 && b.maxY < 69, "\(b)")
        XCTAssertTrue(close(b.minX, 0) && close(b.maxX, 200) && close(b.minY, 0), "\(b)")
        // Straight edge at x = W - 22, base 6 wide toward the start.
        XCTAssertTrue(p.contains(CGPoint(x: 177.5, y: 62)))
        XCTAssertFalse(p.contains(CGPoint(x: 178.5, y: 62)))
        XCTAssertFalse(p.contains(CGPoint(x: 172.5, y: 62)))
        let rtl = shape(.downLeft)
        XCTAssertTrue(rtl.contains(CGPoint(x: 22.5, y: 62)))
        XCTAssertFalse(rtl.contains(CGPoint(x: 21.5, y: 62)))
    }

    func testTheOutlineIsOneSubpath() {
        for t in [BubbleTail.left, .right, .downRight, .downLeft] {
            var moves = 0
            shape(t).forEach { if case .move = $0 { moves += 1 } }
            XCTAssertEqual(moves, 1, "\(t)")
        }
    }
}
