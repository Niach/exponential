import CoreGraphics
import ExpCore
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Core Graphics port of packages/widget/src/annotate/{draw,flatten}.ts. Shared
/// by the live overlay (AnnotationCanvasNSView) and the final flatten, so what
/// you draw is exactly what ships. All shape coords are in IMAGE pixel space;
/// the overlay applies its own display-scale transform before calling in.
enum MacAnnotationRenderer {
    // marker.io-style red: high contrast on most screenshots, matches the
    // widget's destructive token (#ef4444).
    static let annotationColor = CGColor(red: 0xEF / 255.0, green: 0x44 / 255.0, blue: 0x44 / 255.0, alpha: 1)

    // MARK: - Draw

    /// Draw every shape into `context` at the given per-image stroke width. The
    /// caller sets up the coordinate space (image pixels, y-down to match the
    /// canvas) — see AnnotationCanvasNSView / flatten below.
    static func drawShapes(
        in context: CGContext,
        shapes: [AnnotationShape],
        strokeWidth: CGFloat
    ) {
        for shape in shapes { drawShape(in: context, shape: shape, strokeWidth: strokeWidth) }
    }

    static func drawShape(
        in context: CGContext,
        shape: AnnotationShape,
        strokeWidth: CGFloat
    ) {
        setStroke(context, strokeWidth: strokeWidth)
        switch shape.tool {
        case .pen: drawPen(in: context, shape: shape, strokeWidth: strokeWidth)
        case .rect: drawRect(in: context, shape: shape)
        case .arrow: drawArrow(in: context, shape: shape, strokeWidth: strokeWidth)
        }
    }

    private static func setStroke(_ context: CGContext, strokeWidth: CGFloat) {
        context.setStrokeColor(annotationColor)
        context.setFillColor(annotationColor)
        context.setLineWidth(strokeWidth)
        context.setLineCap(.round)
        context.setLineJoin(.round)
    }

    private static func cg(_ p: Point) -> CGPoint { CGPoint(x: p.x, y: p.y) }

    private static func drawPen(in context: CGContext, shape: AnnotationShape, strokeWidth: CGFloat) {
        let points = shape.points
        if points.isEmpty { return }
        if points.count == 1 {
            // In-progress stroke before the first move: a dot.
            let c = cg(points[0])
            let r = strokeWidth / 2
            context.fillEllipse(in: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2))
            return
        }
        // Quadratic midpoint smoothing keeps fast strokes from looking like
        // polygons without resampling the input (matches draw.ts exactly).
        context.beginPath()
        context.move(to: cg(points[0]))
        var i = 1
        while i < points.count - 1 {
            let mid = CGPoint(
                x: (points[i].x + points[i + 1].x) / 2,
                y: (points[i].y + points[i + 1].y) / 2
            )
            context.addQuadCurve(to: mid, control: cg(points[i]))
            i += 1
        }
        context.addLine(to: cg(points[points.count - 1]))
        context.strokePath()
    }

    private static func drawRect(in context: CGContext, shape: AnnotationShape) {
        guard shape.points.count >= 2 else { return }
        let rect = AnnotationGeometry.normalizeRect(shape.points[0], shape.points[1])
        context.stroke(CGRect(x: rect.x, y: rect.y, width: rect.width, height: rect.height))
    }

    private static func drawArrow(in context: CGContext, shape: AnnotationShape, strokeWidth: CGFloat) {
        guard shape.points.count >= 2 else { return }
        let from = shape.points[0]
        let to = shape.points[1]
        let head = AnnotationGeometry.arrowHead(from: from, to: to, strokeWidth: Double(strokeWidth))
        // Shaft (stops short so the line doesn't poke through the filled head).
        context.beginPath()
        context.move(to: cg(from))
        context.addLine(to: cg(head.shaftEnd))
        context.strokePath()
        // Filled triangle head.
        context.beginPath()
        context.move(to: cg(head.tip))
        context.addLine(to: cg(head.left))
        context.addLine(to: cg(head.right))
        context.closePath()
        context.fillPath()
    }

    // MARK: - Flatten

    private static let preferredMaxBytes = 5 * 1024 * 1024
    // Stay under the server's 10MB attachment cap with multipart headroom.
    private static let hardMaxBytes = 9 * 1024 * 1024

    /// Encoded annotated image + filename. The base image is decoded into a
    /// fresh bitmap (top-left origin to match canvas/image space), the shapes
    /// are baked in at the per-image stroke width, and the result is re-encoded
    /// via the WebP -> JPEG ladder. Returns nil on any decode/encode failure so
    /// the caller can keep the unannotated screenshot — never blocks submission.
    static func flatten(
        baseImage: CGImage,
        shapes: [AnnotationShape]
    ) -> (data: Data, filename: String)? {
        let width = baseImage.width
        let height = baseImage.height
        guard width > 0, height > 0 else { return nil }

        let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()
        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }

        // CGContext is y-up; flip so shape coords (y-down image space, matching
        // the HTML canvas) draw at the correct row.
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: 1, y: -1)
        context.draw(baseImage, in: CGRect(x: 0, y: 0, width: width, height: height))

        if !shapes.isEmpty {
            let stroke = AnnotationGeometry.strokeWidthFor(
                imageWidth: Double(width),
                imageHeight: Double(height)
            )
            drawShapes(in: context, shapes: shapes, strokeWidth: CGFloat(stroke))
        }

        guard let flattened = context.makeImage() else { return nil }
        return encode(flattened)
    }

    /// WebP 0.9 -> JPEG 0.8 ladder mirroring capture/image.ts encodeScreenshot:
    /// prefer the smaller acceptable encoding; bail (nil) only if even JPEG
    /// exceeds the hard cap. Older macOS without WebP encode support falls
    /// straight through to JPEG.
    private static func encode(_ image: CGImage) -> (data: Data, filename: String)? {
        let webp = encode(image, type: webpType, quality: 0.9)
        if let webp, webp.count <= preferredMaxBytes {
            return (webp, "screenshot.webp")
        }
        let jpeg = encode(image, type: UTType.jpeg, quality: 0.8)
        // Pick the smaller of the two successful encodings (as the TS ladder does).
        var best: (data: Data, filename: String)?
        if let webp { best = (webp, "screenshot.webp") }
        if let jpeg, jpeg.count < (best?.data.count ?? Int.max) {
            best = (jpeg, "screenshot.jpg")
        }
        guard let best, best.data.count <= hardMaxBytes else { return nil }
        return best
    }

    // WebP UTType is only exposed by name on some SDKs — resolve it dynamically
    // so the file compiles on any toolchain (nil => skip WebP, use JPEG).
    private static let webpType: UTType? = UTType("org.webmproject.webp")

    private static func encode(_ image: CGImage, type: UTType?, quality: CGFloat) -> Data? {
        guard let type else { return nil }
        let data = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(
            data, type.identifier as CFString, 1, nil
        ) else { return nil }
        let options: [CFString: Any] = [kCGImageDestinationLossyCompressionQuality: quality]
        CGImageDestinationAddImage(dest, image, options as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { return nil }
        return data as Data
    }

    /// Decode raw image bytes (a captured PNG/JPEG frame) into a CGImage for
    /// flattening or display. Nil on failure.
    static func decode(_ data: Data) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }
}
