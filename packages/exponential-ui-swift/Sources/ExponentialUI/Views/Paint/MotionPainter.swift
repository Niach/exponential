import SwiftUI
import CoreGraphics
import ImageIO

/// EXP-1604: one frame of a Motion, decoded from the packed display list
/// (`apps/desktop/crates/eui-motion/README.md` › Packed frames). The painter
/// paints this and nothing else: no Lottie, no clock.
public struct MotionList: Sendable {
    public enum Brush: Sendable {
        case solid(Color4)
        case linear(start: CGPoint, end: CGPoint, stops: [Stop])
        case radial(center: CGPoint, focal: CGPoint, radius: CGFloat, stops: [Stop])
    }

    public struct Color4: Sendable, Equatable {
        public var r, g, b, a: Double
        var color: Color { Color(.sRGB, red: r, green: g, blue: b, opacity: a) }
        var cg: CGColor { CGColor(srgbRed: r, green: g, blue: b, alpha: a) }
    }

    public struct Stop: Sendable {
        public var offset: Double
        public var color: Color4
    }

    public enum Filter: Sendable {
        case blur(sigmaX: Double, sigmaY: Double)
        /// Row-major 4×5, offsets in 0…1 (SVG `feColorMatrix`).
        case colorMatrix([Double])
        case gradientMap(amount: Double, stops: [Stop])
        case dropShadow(Color4, dx: Double, dy: Double, sigma: Double)
    }

    public struct Dash: Sendable {
        public var width: CGFloat
        public var cap: CGLineCap
        public var join: CGLineJoin
        public var miter: CGFloat
        public var offset: CGFloat
        public var dashes: [CGFloat]
    }

    public enum Op: Sendable {
        case layer(blend: Int, alpha: Double, transform: CGAffineTransform, clip: Path, filters: [Filter])
        case clip(transform: CGAffineTransform, path: Path)
        case pop
        case fill(transform: CGAffineTransform, evenOdd: Bool, brush: Brush, path: Path)
        case stroke(transform: CGAffineTransform, style: Dash, brush: Brush, path: Path)
        case image(transform: CGAffineTransform, asset: Int, alpha: Double, width: CGFloat, height: CGFloat)
    }

    public var width: CGFloat
    public var height: CGFloat
    public var ops: [Op]

    /// Decode a packed frame; nil when the stream is not VERSION 1 or ends early.
    public init?(packed p: [Float]) {
        var r = Reader(p: p)
        guard let v = r.next(), v == 1, let w = r.next(), let h = r.next() else { return nil }
        width = CGFloat(w)
        height = CGFloat(h)
        var ops: [Op] = []
        while !r.done {
            guard let op = r.op() else { return nil }
            ops.append(op)
        }
        self.ops = ops
    }

    private struct Reader {
        let p: [Float]
        var i = 0
        var done: Bool { i >= p.count }

        mutating func next() -> Float? {
            guard i < p.count else { return nil }
            defer { i += 1 }
            return p[i]
        }

        mutating func d() -> Double? { next().map(Double.init) }
        mutating func cg() -> CGFloat? { next().map { CGFloat($0) } }
        mutating func n() -> Int? { next().map { Int($0) } }

        mutating func transform() -> CGAffineTransform? {
            guard let a = cg(), let b = cg(), let c = cg(), let d = cg(), let e = cg(), let f = cg() else { return nil }
            // tiny-skia `from_row(sx, ky, kx, sy, tx, ty)` = CG (a, b, c, d, tx, ty).
            return CGAffineTransform(a: a, b: b, c: c, d: d, tx: e, ty: f)
        }

        mutating func path() -> Path? {
            guard let count = n() else { return nil }
            var path = Path()
            for _ in 0..<count {
                guard let verb = n() else { return nil }
                switch verb {
                case 0:
                    guard let x = cg(), let y = cg() else { return nil }
                    path.move(to: CGPoint(x: x, y: y))
                case 1:
                    guard let x = cg(), let y = cg() else { return nil }
                    path.addLine(to: CGPoint(x: x, y: y))
                case 2:
                    guard let x1 = cg(), let y1 = cg(), let x2 = cg(), let y2 = cg(), let x = cg(), let y = cg() else { return nil }
                    path.addCurve(to: CGPoint(x: x, y: y), control1: CGPoint(x: x1, y: y1), control2: CGPoint(x: x2, y: y2))
                case 3:
                    path.closeSubpath()
                default:
                    return nil
                }
            }
            return path
        }

        mutating func color() -> Color4? {
            guard let r = d(), let g = d(), let b = d(), let a = d() else { return nil }
            return Color4(r: r, g: g, b: b, a: a)
        }

        mutating func stops() -> [Stop]? {
            guard let count = n() else { return nil }
            var out: [Stop] = []
            for _ in 0..<count {
                guard let o = d(), let c = color() else { return nil }
                out.append(Stop(offset: o, color: c))
            }
            return out
        }

        mutating func brush() -> Brush? {
            switch n() {
            case 0: return color().map(Brush.solid)
            case 1:
                guard let x0 = cg(), let y0 = cg(), let x1 = cg(), let y1 = cg(), let s = stops() else { return nil }
                return .linear(start: CGPoint(x: x0, y: y0), end: CGPoint(x: x1, y: y1), stops: s)
            case 2:
                guard let cx = cg(), let cy = cg(), let fx = cg(), let fy = cg(), let r = cg(), let s = stops() else { return nil }
                return .radial(center: CGPoint(x: cx, y: cy), focal: CGPoint(x: fx, y: fy), radius: r, stops: s)
            default: return nil
            }
        }

        mutating func filter() -> Filter? {
            switch n() {
            case 0:
                guard let x = d(), let y = d() else { return nil }
                return .blur(sigmaX: x, sigmaY: y)
            case 1:
                var m: [Double] = []
                for _ in 0..<20 { guard let v = d() else { return nil }; m.append(v) }
                return .colorMatrix(m)
            case 2:
                guard let amount = d(), let s = stops() else { return nil }
                return .gradientMap(amount: amount, stops: s)
            case 3:
                guard let c = color(), let dx = d(), let dy = d(), let sigma = d() else { return nil }
                return .dropShadow(c, dx: dx, dy: dy, sigma: sigma)
            default: return nil
            }
        }

        mutating func op() -> Op? {
            switch n() {
            case 1:
                guard let blend = n(), let alpha = d(), let t = transform(), let clip = path(), let count = n() else { return nil }
                var filters: [Filter] = []
                for _ in 0..<count { guard let f = filter() else { return nil }; filters.append(f) }
                return .layer(blend: blend, alpha: alpha, transform: t, clip: clip, filters: filters)
            case 2:
                guard let t = transform(), let path = path() else { return nil }
                return .clip(transform: t, path: path)
            case 3: return .pop
            case 4:
                guard let t = transform(), let rule = n(), let brush = brush(), let path = path() else { return nil }
                return .fill(transform: t, evenOdd: rule == 1, brush: brush, path: path)
            case 5:
                guard let t = transform(), let width = cg(), let cap = n(), let join = n(), let miter = cg(), let offset = cg(), let count = n() else { return nil }
                var dashes: [CGFloat] = []
                for _ in 0..<count { guard let v = cg() else { return nil }; dashes.append(v) }
                guard let brush = brush(), let path = path() else { return nil }
                let style = Dash(
                    width: width,
                    cap: cap == 1 ? .round : cap == 2 ? .square : .butt,
                    join: join == 1 ? .round : join == 2 ? .bevel : .miter,
                    miter: miter, offset: offset, dashes: dashes
                )
                return .stroke(transform: t, style: style, brush: brush, path: path)
            case 6:
                guard let t = transform(), let asset = n(), let alpha = d(), let w = cg(), let h = cg() else { return nil }
                return .image(transform: t, asset: asset, alpha: alpha, width: w, height: h)
            default: return nil
            }
        }
    }
}

/// EXP-1604: paints a `MotionList` into a `GraphicsContext` (contract §5,
/// the `raster` reference is the truth): every `layer` an offscreen group
/// under its clip, composited on `pop` with its alpha, blend and filters.
public enum MotionPainter {
    /// The fit transform of a `w × h` composition into `box` (§4).
    public static func fit(width w: CGFloat, height h: CGFloat, into box: CGSize, mode: String) -> CGAffineTransform {
        guard w > 0, h > 0 else { return .identity }
        let sx = box.width / w, sy = box.height / h
        switch mode {
        case "fill":
            return CGAffineTransform(a: sx, b: 0, c: 0, d: sy, tx: 0, ty: 0)
        case "cover":
            let s = max(sx, sy)
            return CGAffineTransform(a: s, b: 0, c: 0, d: s, tx: (box.width - w * s) / 2, ty: (box.height - h * s) / 2)
        default:
            let s = min(sx, sy)
            return CGAffineTransform(a: s, b: 0, c: 0, d: s, tx: (box.width - w * s) / 2, ty: (box.height - h * s) / 2)
        }
    }

    /// Paint `list` into `box` (clipped to it) with the `fit` transform.
    public static func paint(_ list: MotionList, in ctx: inout GraphicsContext, box: CGSize, fit mode: String = "contain", images: [CGImage?] = []) {
        let base = fit(width: list.width, height: list.height, into: box, mode: mode)
        var g = ctx
        g.clip(to: Path(CGRect(origin: .zero, size: box)))
        g.concatenate(base)
        let scale = sqrt(abs(base.a * base.d - base.b * base.c))
        var i = 0
        run(list.ops, &i, &g, scale: scale, images: images)
    }

    /// Paint ops from `i` until the `pop` that closes the current group.
    private static func run(_ ops: [MotionList.Op], _ i: inout Int, _ ctx: inout GraphicsContext, scale: CGFloat, images: [CGImage?]) {
        while i < ops.count {
            let op = ops[i]
            i += 1
            switch op {
            case .pop:
                return
            case let .clip(t, path):
                var g = ctx
                g.clip(to: path.applying(t))
                run(ops, &i, &g, scale: scale, images: images)
            case let .layer(blend, alpha, t, clip, filters):
                var g = ctx
                g.clip(to: clip.applying(t))
                g.opacity = alpha
                g.blendMode = blendMode(blend)
                for f in filters { addFilter(f, to: &g, scale: scale) }
                let luma = blend == 18 || blend == 19
                let bounds = clip.applying(t).boundingRect
                // The group as one draw op of `g`: its alpha, blend and
                // filters apply to the composited layer, never per draw.
                g.drawLayer { inner in
                    inner.opacity = 1
                    inner.blendMode = .normal
                    if luma {
                        // The matte over opaque black, then luminance → alpha:
                        // Rec. 709 of the PREMULTIPLIED colour (the raster's
                        // `luma_mask`, the matte over transparent black).
                        inner.addFilter(.luminanceToAlpha)
                        inner.drawLayer { m in
                            m.fill(Path(bounds.insetBy(dx: -1, dy: -1)), with: .color(.black))
                            run(ops, &i, &m, scale: scale, images: images)
                        }
                    } else {
                        run(ops, &i, &inner, scale: scale, images: images)
                    }
                }
            case let .fill(t, evenOdd, brush, path):
                var g = ctx
                g.concatenate(t)
                fill(path, brush: brush, evenOdd: evenOdd, in: &g)
            case let .stroke(t, style, brush, path):
                var g = ctx
                g.concatenate(t)
                var dashes = style.dashes
                if dashes.count % 2 == 1 { dashes += dashes }
                if !dashes.contains(where: { $0 > 0 }) || dashes.contains(where: { $0 < 0 }) { dashes = [] }
                let s = StrokeStyle(lineWidth: style.width, lineCap: style.cap, lineJoin: style.join, miterLimit: style.miter, dash: dashes, dashPhase: dashes.isEmpty ? 0 : style.offset)
                switch brush {
                case .radial(let center, let focal, let radius, let stops) where center != focal:
                    // A stroke with a focal gradient: its outline filled.
                    let outline = path.strokedPath(s)
                    conical(outline, center: center, focal: focal, radius: radius, stops: stops, evenOdd: false, in: &g)
                default:
                    guard let shading = shading(brush) else { continue }
                    g.stroke(path, with: shading, style: s)
                }
            case let .image(t, asset, alpha, w, h):
                guard asset >= 0, asset < images.count, let img = images[asset] else { continue }
                var g = ctx
                g.concatenate(t)
                g.opacity = alpha
                let iw = CGFloat(img.width), ih = CGFloat(img.height)
                let rect = CGRect(x: 0, y: 0, width: w > 0 ? w : iw, height: h > 0 ? h : ih)
                g.draw(Image(decorative: img, scale: 1), in: rect)
            }
        }
    }

    private static func fill(_ path: Path, brush: MotionList.Brush, evenOdd: Bool, in g: inout GraphicsContext) {
        if case let .radial(center, focal, radius, stops) = brush, center != focal {
            conical(path, center: center, focal: focal, radius: radius, stops: stops, evenOdd: evenOdd, in: &g)
            return
        }
        guard let shading = shading(brush) else { return }
        g.fill(path, with: shading, style: FillStyle(eoFill: evenOdd, antialiased: true))
    }

    private static func gradient(_ stops: [MotionList.Stop]) -> Gradient {
        Gradient(stops: straight(stops).map { Gradient.Stop(color: $0.color.color, location: $0.offset) })
    }

    /// The raster (Skia) interpolates gradient colours UNpremultiplied, Core
    /// Graphics premultiplied: where alpha changes between two stops, 16
    /// stops in between carry the straight interpolation (≤ 1/255 apart).
    static func straight(_ stops: [MotionList.Stop]) -> [MotionList.Stop] {
        guard stops.count > 1 else { return stops }
        var out: [MotionList.Stop] = [stops[0]]
        for (a, b) in zip(stops, stops.dropFirst()) {
            if a.color.a != b.color.a, b.offset > a.offset {
                for k in 1..<16 {
                    let t = Double(k) / 16
                    let c = MotionList.Color4(r: a.color.r + (b.color.r - a.color.r) * t, g: a.color.g + (b.color.g - a.color.g) * t,
                                              b: a.color.b + (b.color.b - a.color.b) * t, a: a.color.a + (b.color.a - a.color.a) * t)
                    out.append(MotionList.Stop(offset: a.offset + (b.offset - a.offset) * t, color: c))
                }
            }
            out.append(b)
        }
        return out
    }

    private static func shading(_ brush: MotionList.Brush) -> GraphicsContext.Shading? {
        switch brush {
        case let .solid(c):
            return .color(c.color)
        case let .linear(start, end, stops):
            guard !stops.isEmpty else { return nil }
            if start == end { return .color(stops.last!.color.color) }
            return .linearGradient(gradient(stops), startPoint: start, endPoint: end, options: [])
        case let .radial(center, _, radius, stops):
            guard !stops.isEmpty, radius > 0 else { return nil }
            return .radialGradient(gradient(stops), center: center, startRadius: 0, endRadius: radius, options: [])
        }
    }

    /// Two-point conical (focal circle radius 0 → the centre circle), which
    /// SwiftUI's `radialGradient` lacks: Core Graphics draws it, clipped to
    /// the path, padded both ways like the raster's `SpreadMode::Pad`.
    private static func conical(_ path: Path, center: CGPoint, focal: CGPoint, radius: CGFloat, stops: [MotionList.Stop], evenOdd: Bool, in g: inout GraphicsContext) {
        guard !stops.isEmpty, radius > 0,
              let cg = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: straight(stops).map(\.color.cg) as CFArray, locations: straight(stops).map { CGFloat($0.offset) })
        else { return }
        var c = g
        c.clip(to: path, style: FillStyle(eoFill: evenOdd))
        c.withCGContext { ctx in
            ctx.drawRadialGradient(cg, startCenter: focal, startRadius: 0, endCenter: center, endRadius: radius, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
        }
    }

    static func blendMode(_ b: Int) -> GraphicsContext.BlendMode {
        switch b {
        case 1: .multiply
        case 2: .screen
        case 3: .overlay
        case 4: .darken
        case 5: .lighten
        case 6: .colorDodge
        case 7: .colorBurn
        case 8: .hardLight
        case 9: .softLight
        case 10: .difference
        case 11: .exclusion
        case 12: .hue
        case 13: .saturation
        case 14: .color
        case 15: .luminosity
        case 16, 18: .destinationIn
        case 17, 19: .destinationOut
        case 20: .xor
        default: .normal
        }
    }

    /// The group's filters, sigmas and offsets × the fit scale (the context
    /// is already under the fit transform, so offsets stay in composition
    /// units; blur radii are in points).
    private static func addFilter(_ f: MotionList.Filter, to g: inout GraphicsContext, scale: CGFloat) {
        switch f {
        case let .blur(sx, sy):
            let r = max(sx, sy) * scale
            if r > 0 { g.addFilter(.blur(radius: r)) }
        case let .colorMatrix(m):
            g.addFilter(.colorMatrix(colorMatrix(m)))
        case let .gradientMap(amount, stops):
            // The map's end stops (luminance 0 → first, 1 → last) as one
            // colour matrix, mixed with the source by `amount`.
            guard let lo = stops.first?.color, let hi = stops.last?.color else { return }
            let l = [0.2125, 0.7154, 0.0721]
            var m = [Double](repeating: 0, count: 20)
            let lows = [lo.r, lo.g, lo.b], highs = [hi.r, hi.g, hi.b]
            for row in 0..<3 {
                for col in 0..<3 {
                    m[row * 5 + col] = amount * l[col] * (highs[row] - lows[row]) + (row == col ? 1 - amount : 0)
                }
                m[row * 5 + 4] = amount * lows[row]
            }
            m[18] = 1
            g.addFilter(.colorMatrix(colorMatrix(m)))
        case let .dropShadow(c, dx, dy, sigma):
            g.addFilter(.shadow(color: c.color, radius: sigma * scale, x: dx, y: dy, blendMode: .normal, options: []))
        }
    }

    private static func colorMatrix(_ m: [Double]) -> ColorMatrix {
        var c = ColorMatrix()
        let f = m.map { Float($0) }
        c.r1 = f[0]; c.r2 = f[1]; c.r3 = f[2]; c.r4 = f[3]; c.r5 = f[4]
        c.g1 = f[5]; c.g2 = f[6]; c.g3 = f[7]; c.g4 = f[8]; c.g5 = f[9]
        c.b1 = f[10]; c.b2 = f[11]; c.b3 = f[12]; c.b4 = f[13]; c.b5 = f[14]
        c.a1 = f[15]; c.a2 = f[16]; c.a3 = f[17]; c.a4 = f[18]; c.a5 = f[19]
        return c
    }

    /// Decode an image asset location: a `data:` URL (base64) into a CGImage.
    public static func decodeDataURL(_ location: String) -> CGImage? {
        guard location.hasPrefix("data:"), let comma = location.firstIndex(of: ",") else { return nil }
        let meta = location[location.startIndex..<comma]
        let body = String(location[location.index(after: comma)...])
        let data: Data?
        if meta.hasSuffix(";base64") {
            data = Data(base64Encoded: body, options: .ignoreUnknownCharacters)
        } else {
            data = body.removingPercentEncoding.flatMap { $0.data(using: .utf8) }
        }
        guard let data, let src = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(src, 0, nil)
    }
}
