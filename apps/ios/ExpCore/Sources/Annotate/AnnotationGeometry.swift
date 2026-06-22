import Foundation

// A faithful port of packages/widget/src/annotate/shapes.ts. All shape
// coordinates live in IMAGE pixel space (the screenshot's native raster):
// flattening is exact and the editor's display scale is a pure transform.
// The cross-language parity gate (AnnotationGeometryTests / shapes.test.ts)
// locks `normalizeRect` / `strokeWidthFor` / `arrowHead` / `isDegenerate`
// to byte-identical results across web, iOS/macOS and Linux.

public enum AnnotationTool: String, Codable, CaseIterable, Sendable {
    case rect
    case pen
    case arrow
}

public struct Point: Equatable, Sendable {
    public var x: Double
    public var y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }
}

public struct AnnotationShape: Equatable, Sendable {
    public let tool: AnnotationTool
    // rect/arrow: [start, end]. pen: the sampled polyline (>=1 point while
    // drafting; committed shapes pass isDegenerate first).
    public var points: [Point]

    public init(tool: AnnotationTool, points: [Point]) {
        self.tool = tool
        self.points = points
    }
}

public struct NormalizedRect: Equatable, Sendable {
    public let x: Double
    public let y: Double
    public let width: Double
    public let height: Double

    public init(x: Double, y: Double, width: Double, height: Double) {
        self.x = x
        self.y = y
        self.width = width
        self.height = height
    }
}

public struct ArrowHead: Equatable, Sendable {
    // Where the shaft stroke should stop so it doesn't poke through the head.
    public let shaftEnd: Point
    public let tip: Point
    public let left: Point
    public let right: Point

    public init(shaftEnd: Point, tip: Point, left: Point, right: Point) {
        self.shaftEnd = shaftEnd
        self.tip = tip
        self.left = left
        self.right = right
    }
}

public enum AnnotationGeometry {
    // Corner pair -> origin + positive extent, whatever the drag direction.
    public static func normalizeRect(_ a: Point, _ b: Point) -> NormalizedRect {
        NormalizedRect(
            x: min(a.x, b.x),
            y: min(a.y, b.y),
            width: abs(a.x - b.x),
            height: abs(a.y - b.y)
        )
    }

    // One stroke width per image (~0.35% of the long edge, min 3px) so all
    // shapes on a screenshot read consistently at both full and thumbnail size.
    public static func strokeWidthFor(imageWidth: Double, imageHeight: Double) -> Double {
        let longest = max(imageWidth, imageHeight, 1)
        return max(3, (longest * 0.0035).rounded())
    }

    private static let arrowSpread = Double.pi / 7

    public static func arrowHeadLength(strokeWidth: Double) -> Double {
        max(10, strokeWidth * 4)
    }

    // Filled-triangle head for an arrow from `from` to `to`.
    public static func arrowHead(from: Point, to: Point, strokeWidth: Double) -> ArrowHead {
        let angle = atan2(to.y - from.y, to.x - from.x)
        let length = arrowHeadLength(strokeWidth: strokeWidth)
        func wing(_ spread: Double) -> Point {
            Point(
                x: to.x - length * cos(angle + spread),
                y: to.y - length * sin(angle + spread)
            )
        }
        return ArrowHead(
            shaftEnd: Point(
                x: to.x - length * 0.6 * cos(angle),
                y: to.y - length * 0.6 * sin(angle)
            ),
            tip: to,
            left: wing(-arrowSpread),
            right: wing(arrowSpread)
        )
    }

    public static func distance(_ a: Point, _ b: Point) -> Double {
        hypot(a.x - b.x, a.y - b.y)
    }

    // Accidental clicks shouldn't commit invisible shapes: rect/arrow need a
    // real drag; a pen stroke needs at least two samples (a tiny scribble is a
    // legitimate dot marker, so no length threshold there).
    public static func isDegenerate(_ shape: AnnotationShape, minDragPx: Double = 4) -> Bool {
        if shape.tool == .pen { return shape.points.count < 2 }
        if shape.points.count < 2 { return true }
        return distance(shape.points[0], shape.points[1]) < minDragPx
    }

    public static func clampPoint(_ point: Point, width: Double, height: Double) -> Point {
        Point(
            x: min(max(point.x, 0), width),
            y: min(max(point.y, 0), height)
        )
    }
}
