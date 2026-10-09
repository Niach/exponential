import SwiftUI
import ExponentialUI
#if canImport(UIKit)
import UIKit

/// Renders the WHOLE surface (not the viewport) into a PNG at a host path:
/// a window of its own as tall as the content, drawn through Core
/// Animation so the TextKit labels paint (an `ImageRenderer` would leave
/// them blank, `drawHierarchy` only what is on screen).
@MainActor
enum FullSurfaceDump {
    static func write(model: SurfaceModel, to path: String) async {
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first else { return }
        let width = scene.screen.bounds.width
        let height = max(model.surfaceSize.height, 10)
        let background = model.color("background") ?? .black
        let host = UIHostingController(rootView: ExponentialSurface(model: model).frame(width: width, height: height, alignment: .topLeading).background(background))
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: width, height: height)
        window.rootViewController = host
        window.overrideUserInterfaceStyle = model.mode == .dark ? .dark : .light
        window.isHidden = false
        window.layoutIfNeeded()
        // SwiftUI lays the hosting view out on the next runloop turns.
        try? await Task.sleep(for: .milliseconds(1500))
        window.layoutIfNeeded()
        let format = UIGraphicsImageRendererFormat()
        format.scale = 2
        let renderer = UIGraphicsImageRenderer(size: window.bounds.size, format: format)
        let image = renderer.image { ctx in window.layer.render(in: ctx.cgContext) }
        window.isHidden = true
        try? image.pngData()?.write(to: URL(fileURLWithPath: path))
    }
}
#else
enum FullSurfaceDump {
    static func write(model: SurfaceModel, to path: String) async {}
}
#endif
