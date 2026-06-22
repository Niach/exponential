import AppKit
import ExpCore
import ExpUI
import SwiftUI

/// The marker.io-style annotate overlay shown over the live preview. Freezes a
/// clean frame (image-pixel space), lets the user draw rect/pen/arrow in fixed
/// red, and flattens the result for a feedback report. All shape coords live in
/// image-pixel space; the canvas maps mouse → image space and back for drawing.
@MainActor
@Observable
final class AnnotationModel {
    /// The frozen base frame being annotated, in its native pixel size.
    let baseImage: CGImage
    var tool: AnnotationTool = .rect
    private(set) var shapes: [AnnotationShape] = []
    // The in-progress shape while dragging (committed on mouse-up if non-degenerate).
    private(set) var draft: AnnotationShape?

    var imageWidth: CGFloat { CGFloat(baseImage.width) }
    var imageHeight: CGFloat { CGFloat(baseImage.height) }

    init(baseImage: CGImage) { self.baseImage = baseImage }

    func beginDrag(at point: Point) {
        let clamped = AnnotationGeometry.clampPoint(point, width: Double(imageWidth), height: Double(imageHeight))
        draft = AnnotationShape(tool: tool, points: [clamped])
    }

    func continueDrag(to point: Point) {
        guard var draft else { return }
        let clamped = AnnotationGeometry.clampPoint(point, width: Double(imageWidth), height: Double(imageHeight))
        if draft.tool == .pen {
            draft.points.append(clamped)
        } else {
            // rect/arrow: keep [start, current].
            if draft.points.count < 2 { draft.points.append(clamped) }
            else { draft.points[1] = clamped }
        }
        self.draft = draft
    }

    func endDrag() {
        defer { draft = nil }
        guard let draft else { return }
        if !AnnotationGeometry.isDegenerate(draft) { shapes.append(draft) }
    }

    /// All shapes including the in-progress draft (for live drawing).
    var renderableShapes: [AnnotationShape] {
        draft.map { shapes + [$0] } ?? shapes
    }

    func undo() { if !shapes.isEmpty { shapes.removeLast() } }
    func clear() { shapes.removeAll(); draft = nil }

    var hasShapes: Bool { !shapes.isEmpty }

    /// Flatten the annotations into the base frame, encoded for upload. nil on
    /// encode failure (the caller keeps the unannotated frame / aborts the shot).
    func flatten() -> (data: Data, filename: String)? {
        MacAnnotationRenderer.flatten(baseImage: baseImage, shapes: shapes)
    }
}

/// The drawing surface: shows the frozen frame scaled to fit and renders shapes
/// in image space via MacAnnotationRenderer. Mouse points are mapped overlay →
/// image space (the inverse of the fit transform).
final class AnnotationCanvasNSView: NSView {
    private let model: AnnotationModel

    init(model: AnnotationModel) {
        self.model = model
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    override var isFlipped: Bool { true } // top-left origin to match image space

    // The rect (in view coords) the image is drawn into, preserving aspect.
    private var imageRect: CGRect {
        let iw = model.imageWidth, ih = model.imageHeight
        guard iw > 0, ih > 0, bounds.width > 0, bounds.height > 0 else { return bounds }
        let scale = min(bounds.width / iw, bounds.height / ih)
        let w = iw * scale, h = ih * scale
        return CGRect(x: (bounds.width - w) / 2, y: (bounds.height - h) / 2, width: w, height: h)
    }

    private func imageScale() -> CGFloat {
        let rect = imageRect
        guard model.imageWidth > 0 else { return 1 }
        return rect.width / model.imageWidth
    }

    // View point → image-pixel space.
    private func toImageSpace(_ p: NSPoint) -> Point {
        let rect = imageRect
        let scale = imageScale()
        guard scale > 0 else { return Point(x: 0, y: 0) }
        return Point(x: Double((p.x - rect.minX) / scale), y: Double((p.y - rect.minY) / scale))
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        let rect = imageRect
        let scale = imageScale()
        let imageW = model.imageWidth, imageH = model.imageHeight

        // Map image-pixel space (top-left origin, y-down — same as the mouse
        // mapping) onto `imageRect`. This view is flipped (top-left, y-down), so
        // after this transform 1 unit == 1 image pixel and shapes overlay exactly.
        context.saveGState()
        context.translateBy(x: rect.minX, y: rect.minY)
        context.scaleBy(x: scale, y: scale)

        // The base frame: CGContext.draw assumes y-up, so flip locally within the
        // image box to render it upright in this y-down space.
        context.saveGState()
        context.translateBy(x: 0, y: imageH)
        context.scaleBy(x: 1, y: -1)
        context.draw(model.baseImage, in: CGRect(x: 0, y: 0, width: imageW, height: imageH))
        context.restoreGState()

        // Shapes in image space (already y-down here).
        let stroke = AnnotationGeometry.strokeWidthFor(
            imageWidth: Double(imageW), imageHeight: Double(imageH)
        )
        MacAnnotationRenderer.drawShapes(in: context, shapes: model.renderableShapes, strokeWidth: CGFloat(stroke))
        context.restoreGState()
    }

    // MARK: - Mouse → image-space shapes

    override func mouseDown(with event: NSEvent) {
        model.beginDrag(at: toImageSpace(convert(event.locationInWindow, from: nil)))
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        model.continueDrag(to: toImageSpace(convert(event.locationInWindow, from: nil)))
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        model.endDrag()
        needsDisplay = true
    }
}

/// SwiftUI wrapper for the canvas; redraws when the shape list / tool changes.
struct AnnotationCanvas: NSViewRepresentable {
    let model: AnnotationModel

    func makeNSView(context: Context) -> AnnotationCanvasNSView {
        AnnotationCanvasNSView(model: model)
    }

    func updateNSView(_ view: AnnotationCanvasNSView, context: Context) {
        // Touch the observable so SwiftUI re-runs updateNSView on changes, then
        // force a redraw (the canvas owns the same model reference).
        _ = model.renderableShapes.count
        _ = model.tool
        view.needsDisplay = true
    }
}

/// The annotate overlay: the frozen-frame canvas + a floating toolbar
/// (rect/pen/arrow/undo/clear + Send/Cancel). Lives above the preview pane while
/// `controller.annotating` is true.
struct MacPreviewAnnotateOverlay: View {
    @Bindable var model: AnnotationModel
    let onSend: () -> Void
    let onCancel: () -> Void

    var body: some View {
        ZStack(alignment: .top) {
            AnnotationCanvas(model: model)
            toolbar
                .padding(.top, 10)
        }
        .background(Color.black.opacity(0.001)) // capture clicks over the pane
    }

    private var toolbar: some View {
        HStack(spacing: 6) {
            toolButton(.rect, system: "rectangle", help: "Rectangle")
            toolButton(.pen, system: "scribble", help: "Pen")
            toolButton(.arrow, system: "arrow.up.right", help: "Arrow")
            Divider().frame(height: 18)
            Button { model.undo() } label: { Image(systemName: "arrow.uturn.backward") }
                .help("Undo").disabled(!model.hasShapes)
            Button { model.clear() } label: { Image(systemName: "trash") }
                .help("Clear").disabled(!model.hasShapes)
            Divider().frame(height: 18)
            Button("Cancel", role: .cancel) { onCancel() }
            Button("Send Feedback") { onSend() }
                .keyboardShortcut(.return, modifiers: [.command])
                .buttonStyle(.borderedProminent)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.regularMaterial, in: Capsule())
        .overlay(Capsule().strokeBorder(Color.white.opacity(0.08)))
        .shadow(color: .black.opacity(0.3), radius: 12, y: 4)
    }

    private func toolButton(_ tool: AnnotationTool, system: String, help: String) -> some View {
        Button { model.tool = tool } label: {
            Image(systemName: system)
                .foregroundStyle(model.tool == tool ? Color.accentColor : Color.primary)
        }
        .help(help)
    }
}
