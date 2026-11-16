import SwiftUI

/// #EXP-1876: where a speech-bubble tail grows out of its box, PHYSICAL
/// (the style key `tail` = `start | end | down`, resolved against the
/// direction by `resolve`).
public enum BubbleTail: Equatable, Sendable {
    /// Out of the top-left corner, pointing left.
    case left
    /// Out of the top-right corner, pointing right.
    case right
    /// Down out of the bottom edge, its tip 22 pt in from the right edge.
    case downRight
    /// The mirror: its tip 22 pt in from the left edge.
    case downLeft

    /// The style value (`start | end | down`, anything else = none) in a
    /// direction: rtl mirrors start / end.
    public static func resolve(_ value: String?, rtl: Bool) -> BubbleTail? {
        switch value {
        case "start": rtl ? .right : .left
        case "end": rtl ? .left : .right
        case "down": rtl ? .downLeft : .downRight
        default: nil
        }
    }

    /// The geometry ×4 (style.json `tail`): a sideways tail reaches 6 out
    /// and meets its side 9 below the top; the down tail reaches 9 down,
    /// its straight edge 22 in, its base 6 wide; every tip rounds ~2.
    public static let reach: CGFloat = 6
    public static let sideDrop: CGFloat = 9
    public static let downReach: CGFloat = 9
    public static let downInset: CGFloat = 22
    public static let downBase: CGFloat = 6
}

/// #EXP-1876: a box with per-corner radii (`[topLeft, topRight,
/// bottomRight, bottomLeft]`, circular like CSS) and its speech-bubble tail
/// as ONE outline: fill it with the box's background and stroke it with
/// `strokeInside` so the border runs round box and tail without a seam.
/// The tail lies OUTSIDE the rect it is given (paint-only); the corner a
/// sideways tail grows from is square.
public struct TailedBoxShape: Shape {
    public var radii: [CGFloat]
    public var tail: BubbleTail?

    public init(radii: [CGFloat], tail: BubbleTail?) {
        self.radii = radii.count == 4 ? radii : [0, 0, 0, 0]
        self.tail = tail
    }

    public init(radius: CGFloat, tail: BubbleTail?) {
        self.init(radii: [radius, radius, radius, radius], tail: tail)
    }

    public func path(in rect: CGRect) -> Path {
        let w = rect.width, h = rect.height
        let cap = max(0, min(w, h) / 2)
        var k = radii.map { min(max(0, $0), cap) }
        // Built for the left-hand tails, mirrored in x for the right-hand ones.
        let mirrored = tail == .right || tail == .downLeft
        if mirrored { k = [k[1], k[0], k[3], k[2]] }
        if tail == .left || tail == .right { k[0] = 0 }
        let p = Self.leftHanded(w: w, h: h, k: k, down: tail == .downRight || tail == .downLeft, side: tail == .left || tail == .right)
        var t = CGAffineTransform(translationX: rect.minX, y: rect.minY)
        if mirrored { t = t.translatedBy(x: w, y: 0).scaledBy(x: -1, y: 1) }
        return p.applying(t)
    }

    /// The outline at the origin, clockwise: a `side` tail out of the top
    /// left corner, a `down` one out of the bottom edge near the right.
    static func leftHanded(w: CGFloat, h: CGFloat, k: [CGFloat], down: Bool, side: Bool) -> Path {
        var p = Path()
        let r = BubbleTail.reach
        if side {
            p.move(to: CGPoint(x: -(r - 2), y: 0))
        } else {
            p.move(to: CGPoint(x: k[0], y: 0))
        }
        p.addLine(to: CGPoint(x: w - k[1], y: 0))
        if k[1] > 0 { p.addArc(tangent1End: CGPoint(x: w, y: 0), tangent2End: CGPoint(x: w, y: h), radius: k[1]) }
        p.addLine(to: CGPoint(x: w, y: h - k[2]))
        if k[2] > 0 { p.addArc(tangent1End: CGPoint(x: w, y: h), tangent2End: CGPoint(x: 0, y: h), radius: k[2]) }
        if down {
            let x = w - BubbleTail.downInset, tip = CGPoint(x: x, y: h + BubbleTail.downReach)
            p.addLine(to: CGPoint(x: x, y: h))
            p.addLine(to: CGPoint(x: x, y: tip.y - 2))
            // 2 along the hypotenuse from the tip, toward (x - base, h).
            let (dx, dy) = (-BubbleTail.downBase, -BubbleTail.downReach)
            let len = (dx * dx + dy * dy).squareRoot()
            p.addQuadCurve(to: CGPoint(x: x + dx / len * 2, y: tip.y + dy / len * 2), control: tip)
            p.addLine(to: CGPoint(x: x - BubbleTail.downBase, y: h))
        }
        p.addLine(to: CGPoint(x: k[3], y: h))
        if k[3] > 0 { p.addArc(tangent1End: CGPoint(x: 0, y: h), tangent2End: CGPoint(x: 0, y: 0), radius: k[3]) }
        if side {
            let tip = CGPoint(x: -r, y: 0)
            p.addLine(to: CGPoint(x: 0, y: BubbleTail.sideDrop))
            // 2 along the hypotenuse from the tip, toward (0, sideDrop).
            let len = (r * r + BubbleTail.sideDrop * BubbleTail.sideDrop).squareRoot()
            p.addLine(to: CGPoint(x: tip.x + r / len * 2, y: tip.y + BubbleTail.sideDrop / len * 2))
            p.addQuadCurve(to: CGPoint(x: tip.x + 2, y: 0), control: tip)
        } else {
            p.addLine(to: CGPoint(x: 0, y: k[0]))
            if k[0] > 0 { p.addArc(tangent1End: CGPoint(x: 0, y: 0), tangent2End: CGPoint(x: w, y: 0), radius: k[0]) }
        }
        p.closeSubpath()
        return p
    }
}

public extension Shape {
    /// The border INSIDE the outline like CSS's: the stroke doubled and
    /// clipped to the shape, so a tail's border follows its edge too.
    func strokeInside<S: ShapeStyle>(_ content: S, style: StrokeStyle) -> some View {
        var doubled = style
        doubled.lineWidth *= 2
        return stroke(content, style: doubled).clipShape(self)
    }
}
