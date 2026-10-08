import SwiftUI
import ExponentialUIPrimitives

/// The SwiftUI face of a surface: the root node at the core's frames, as
/// wide as its container and as tall as its content (wrap it in YOUR
/// scroller), with the open layers presented on top. Coordinates are
/// physical (`layoutDirection` pinned), fonts and icons come from the
/// host, the generic primitives read the theme's tokens.
public struct ExponentialSurface: View {
    let model: SurfaceModel

    public init(model: SurfaceModel) {
        self.model = model
    }

    public var body: some View {
        SurfaceBody(model: model)
            .environment(model)
            .environment(\.layoutDirection, .leftToRight)
            .primitiveTokens(model.primitiveTokens)
    }
}

private struct SurfaceBody: View {
    let model: SurfaceModel

    var body: some View {
        let size = model.surfaceSize
        ZStack(alignment: .topLeading) {
            if model.passCount > 0, !model.nodes.isEmpty {
                NodeView(index: 0)
                    .layoutValue(key: NodeIndexKey.self, value: 0)
                PaintedLayers(model: model)
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .frame(height: size.height > 0 ? size.height : nil, alignment: .topLeading)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width in
            model.setViewport(width: width, height: model.viewportHeight, maxHeight: model.maxHeight)
        }
        .modifier(NativeOverlays(model: model))
        .modifier(DatePopup(model: model))
    }
}
