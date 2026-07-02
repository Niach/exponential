import XCTest
@testable import ExpCore

// Cross-language parity gate: a 1:1 port of
// packages/widget/src/annotate/shapes.test.ts. If the geometry diverges from
// the TS source of truth, this fails — keeping the flattened annotation
// pixels identical across web and iOS.
final class AnnotationGeometryTests: XCTestCase {
    private func point(_ x: Double, _ y: Double) -> Point { Point(x: x, y: y) }

    // MARK: - normalizeRect

    func testNormalizeRectKeepsAlreadyNormalizedPair() {
        let rect = AnnotationGeometry.normalizeRect(point(10, 20), point(110, 70))
        XCTAssertEqual(rect, NormalizedRect(x: 10, y: 20, width: 100, height: 50))
    }

    func testNormalizeRectNormalizesADragInAnyDirection() {
        XCTAssertEqual(
            AnnotationGeometry.normalizeRect(point(110, 70), point(10, 20)),
            NormalizedRect(x: 10, y: 20, width: 100, height: 50)
        )
        XCTAssertEqual(
            AnnotationGeometry.normalizeRect(point(10, 70), point(110, 20)),
            NormalizedRect(x: 10, y: 20, width: 100, height: 50)
        )
    }

    // MARK: - strokeWidthFor

    func testStrokeWidthEnforcesThe3pxMinimumOnSmallImages() {
        XCTAssertEqual(AnnotationGeometry.strokeWidthFor(imageWidth: 320, imageHeight: 240), 3)
    }

    func testStrokeWidthScalesWithTheLongEdge() {
        XCTAssertEqual(AnnotationGeometry.strokeWidthFor(imageWidth: 1920, imageHeight: 1080), 7)
        XCTAssertEqual(AnnotationGeometry.strokeWidthFor(imageWidth: 1080, imageHeight: 1920), 7)
    }

    func testStrokeWidthSurvivesDegenerateDimensions() {
        XCTAssertEqual(AnnotationGeometry.strokeWidthFor(imageWidth: 0, imageHeight: 0), 3)
    }

    // MARK: - arrowHead

    func testArrowHead() {
        let from = point(0, 0)
        let to = point(100, 0)
        let strokeWidth: Double = 5
        let head = AnnotationGeometry.arrowHead(from: from, to: to, strokeWidth: strokeWidth)
        let length = AnnotationGeometry.arrowHeadLength(strokeWidth: strokeWidth)
        let accuracy = 1e-6

        // puts the tip at the drag end
        XCTAssertEqual(head.tip, to)

        // places both wings one head-length from the tip
        XCTAssertEqual(AnnotationGeometry.distance(head.left, head.tip), length, accuracy: accuracy)
        XCTAssertEqual(AnnotationGeometry.distance(head.right, head.tip), length, accuracy: accuracy)

        // keeps the wings symmetric about the shaft (shaft on the x-axis)
        XCTAssertEqual(head.left.x, head.right.x, accuracy: accuracy)
        XCTAssertEqual(head.left.y, -head.right.y, accuracy: accuracy)
        XCTAssertFalse(abs(head.left.y) < 0.05) // not ~0

        // pulls the shaft end back inside the head
        XCTAssertEqual(head.shaftEnd.y, 0, accuracy: accuracy)
        XCTAssertLessThan(head.shaftEnd.x, to.x)
        XCTAssertGreaterThan(head.shaftEnd.x, to.x - length)
    }

    func testArrowHeadLengthScalesButNeverBelow10px() {
        XCTAssertEqual(AnnotationGeometry.arrowHeadLength(strokeWidth: 1), 10)
        XCTAssertEqual(AnnotationGeometry.arrowHeadLength(strokeWidth: 8), 32)
    }

    // MARK: - isDegenerate

    func testIsDegenerateDropsClickWithoutDragRectsAndArrows() {
        let rect = AnnotationShape(tool: .rect, points: [point(50, 50), point(52, 51)])
        XCTAssertTrue(AnnotationGeometry.isDegenerate(rect))
        let arrow = AnnotationShape(tool: .arrow, points: rect.points)
        XCTAssertTrue(AnnotationGeometry.isDegenerate(arrow))
    }

    func testIsDegenerateKeepsRealDrags() {
        let arrow = AnnotationShape(tool: .arrow, points: [point(0, 0), point(30, 10)])
        XCTAssertFalse(AnnotationGeometry.isDegenerate(arrow))
    }

    func testIsDegenerateTreatsSinglePointShapesAsDegenerate() {
        XCTAssertTrue(
            AnnotationGeometry.isDegenerate(AnnotationShape(tool: .rect, points: [point(1, 1)]))
        )
    }

    func testIsDegenerateKeepsTinyPenScribblesButDropsSingleSamples() {
        XCTAssertTrue(
            AnnotationGeometry.isDegenerate(AnnotationShape(tool: .pen, points: [point(5, 5)]))
        )
        XCTAssertFalse(
            AnnotationGeometry.isDegenerate(
                AnnotationShape(tool: .pen, points: [point(5, 5), point(6, 5)])
            )
        )
    }

    // MARK: - clampPoint

    func testClampPointClampsToTheImageBounds() {
        XCTAssertEqual(
            AnnotationGeometry.clampPoint(point(-4, 30), width: 100, height: 50),
            point(0, 30)
        )
        XCTAssertEqual(
            AnnotationGeometry.clampPoint(point(120, 60), width: 100, height: 50),
            point(100, 50)
        )
        XCTAssertEqual(
            AnnotationGeometry.clampPoint(point(40, 20), width: 100, height: 50),
            point(40, 20)
        )
    }
}
