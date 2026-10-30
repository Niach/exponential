import SwiftUI

/// VAPP-162: a QR code matrix on its plate (the catalog `QrCode`; the
/// app's pairing code). `rows` = the matrix ('1' dark), no quiet zone: the
/// view adds `quietZone` modules on every side, so a module is
/// `size / (n + 2 × quietZone)` pt, fractional; module EDGES snap to device
/// pixels (`displayScale`) so the code stays crisp and its outer size
/// exact. Nil rows = the empty plate. The colours are taken as given (the
/// renderer already put the lighter one on the plate).
public struct QrCodeView: View {
    let rows: [String]?
    let size: CGFloat
    let plate: Color
    let module: Color
    let radius: CGFloat
    let quietZone: Int
    @Environment(\.displayScale) private var displayScale

    public init(rows: [String]?, size: CGFloat, plate: Color, module: Color, radius: CGFloat = 0, quietZone: Int = 4) {
        self.rows = rows
        self.size = size
        self.plate = plate
        self.module = module
        self.radius = radius
        self.quietZone = quietZone
    }

    public var body: some View {
        Canvas { ctx, _ in
            ctx.fill(Path(roundedRect: CGRect(x: 0, y: 0, width: size, height: size), cornerRadius: radius), with: .color(plate))
            guard let path = Self.modules(rows: rows, size: size, quietZone: quietZone, scale: displayScale) else { return }
            ctx.fill(path, with: .color(module))
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    /// The dark modules as one path: per row, each run of dark modules one
    /// rect whose edges sit on device pixels (`scale` px per pt).
    public static func modules(rows: [String]?, size: CGFloat, quietZone: Int, scale: CGFloat) -> Path? {
        guard let rows, !rows.isEmpty, size > 0 else { return nil }
        let n = rows.count
        let m = size / CGFloat(n + 2 * quietZone)
        let s = max(scale, 1)
        func edge(_ i: Int) -> CGFloat { ((CGFloat(quietZone + i) * m) * s).rounded() / s }
        var p = Path()
        for (y, row) in rows.enumerated() {
            let cells = Array(row.utf8)
            var x = 0
            while x < cells.count {
                guard cells[x] == UInt8(ascii: "1") else { x += 1; continue }
                var end = x
                while end < cells.count, cells[end] == UInt8(ascii: "1") { end += 1 }
                p.addRect(CGRect(x: edge(x), y: edge(y), width: edge(end) - edge(x), height: edge(y + 1) - edge(y)))
                x = end
            }
        }
        return p
    }
}

/// VAPP-162: the four viewfinder brackets of a QR scanner (the catalog
/// `QrScanner`'s `corner` parts): at each inside corner two strokes
/// `length` pt long, `stroke` pt wide, rounded caps, the outer edge
/// following the viewfinder's `radius`.
public struct QrViewfinderCorners: View {
    let length: CGFloat
    let stroke: CGFloat
    let radius: CGFloat
    let color: Color

    public init(length: CGFloat = 24, stroke: CGFloat = 3, radius: CGFloat = 0, color: Color) {
        self.length = length
        self.stroke = stroke
        self.radius = radius
        self.color = color
    }

    public var body: some View {
        Canvas { ctx, size in
            ctx.stroke(Self.path(in: CGRect(origin: .zero, size: size), length: length, stroke: stroke, radius: radius), with: .color(color), style: StrokeStyle(lineWidth: stroke, lineCap: .round, lineJoin: .round))
        }
        .accessibilityHidden(true)
    }

    /// The four brackets, each an L whose corner arcs with the radius
    /// (inset by half the stroke so the stroke stays inside the box).
    public static func path(in rect: CGRect, length: CGFloat, stroke: CGFloat, radius: CGFloat) -> Path {
        let h = stroke / 2
        let r = max(0, min(radius - h, length - h))
        let box = rect.insetBy(dx: h, dy: h)
        var p = Path()
        // top-left
        p.move(to: CGPoint(x: box.minX, y: box.minY + length - h))
        p.addLine(to: CGPoint(x: box.minX, y: box.minY + r))
        if r > 0 { p.addArc(center: CGPoint(x: box.minX + r, y: box.minY + r), radius: r, startAngle: .degrees(180), endAngle: .degrees(270), clockwise: false) }
        p.addLine(to: CGPoint(x: box.minX + length - h, y: box.minY))
        // top-right
        p.move(to: CGPoint(x: box.maxX - length + h, y: box.minY))
        p.addLine(to: CGPoint(x: box.maxX - r, y: box.minY))
        if r > 0 { p.addArc(center: CGPoint(x: box.maxX - r, y: box.minY + r), radius: r, startAngle: .degrees(270), endAngle: .degrees(0), clockwise: false) }
        p.addLine(to: CGPoint(x: box.maxX, y: box.minY + length - h))
        // bottom-right
        p.move(to: CGPoint(x: box.maxX, y: box.maxY - length + h))
        p.addLine(to: CGPoint(x: box.maxX, y: box.maxY - r))
        if r > 0 { p.addArc(center: CGPoint(x: box.maxX - r, y: box.maxY - r), radius: r, startAngle: .degrees(0), endAngle: .degrees(90), clockwise: false) }
        p.addLine(to: CGPoint(x: box.maxX - length + h, y: box.maxY))
        // bottom-left
        p.move(to: CGPoint(x: box.minX + length - h, y: box.maxY))
        p.addLine(to: CGPoint(x: box.minX + r, y: box.maxY))
        if r > 0 { p.addArc(center: CGPoint(x: box.minX + r, y: box.maxY - r), radius: r, startAngle: .degrees(90), endAngle: .degrees(180), clockwise: false) }
        p.addLine(to: CGPoint(x: box.minX, y: box.maxY - length + h))
        return p
    }
}

/// VAPP-162: the QR scanner's `scanLine`: a `height` pt bar inset `inset`
/// pt from the left and right, sweeping top → bottom → top over
/// `period` seconds (ease in-out, alternate) while `moving`; else static at
/// the vertical centre.
public struct QrScanLine: View {
    let color: Color
    let height: CGFloat
    let inset: CGFloat
    let period: Double
    let moving: Bool

    public init(color: Color, height: CGFloat = 2, inset: CGFloat = 16, period: Double = 2, moving: Bool) {
        self.color = color
        self.height = height
        self.inset = inset
        self.period = period
        self.moving = moving
    }

    public var body: some View {
        GeometryReader { geo in
            let travel = max(0, geo.size.height - height)
            TimelineView(.animation(paused: !moving)) { tl in
                let y = moving ? travel * Self.sweep(tl.date.timeIntervalSinceReferenceDate, period: period) : travel / 2
                Capsule()
                    .fill(color)
                    .frame(width: max(0, geo.size.width - 2 * inset), height: height)
                    .offset(x: inset, y: y)
            }
        }
        .accessibilityHidden(true)
    }

    /// The sweep's position 0…1 at `t` seconds: alternate (down, then up),
    /// each leg eased by the standard curve (cubic-bezier 0.2, 0, 0, 1).
    public static func sweep(_ t: Double, period: Double) -> Double {
        guard period > 0 else { return 0.5 }
        let legs = t / period
        let leg = Int(legs.rounded(.down))
        let f = legs - Double(leg)
        let eased = UnitCurve.bezier(startControlPoint: UnitPoint(x: 0.2, y: 0), endControlPoint: UnitPoint(x: 0, y: 1)).value(at: f)
        return leg % 2 == 0 ? eased : 1 - eased
    }
}
