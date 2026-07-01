import AppKit
import SwiftUI

/// Hosts the preview controller's owned embed surface (a WKWebView / capture
/// NSView) inside SwiftUI. Twin of `TerminalDockHost`: the container is what
/// SwiftUI owns/resizes; the embed view is added as a fill subview only while
/// the controller has one AND the container has a nonzero backing size — the
/// same lazy-size discipline the libghostty dock uses, so a stream/webview is
/// never mounted at 0×0.
struct PreviewDockHost: NSViewRepresentable {
    let controller: MacPreviewController

    func makeNSView(context: Context) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.backgroundColor = NSColor.black.cgColor
        return container
    }

    func updateNSView(_ container: NSView, context: Context) {
        let view = controller.embedView
        for sub in container.subviews where sub !== view {
            sub.removeFromSuperview()
        }
        guard controller.isMounted, let view,
              container.bounds.width > 0, container.bounds.height > 0 else { return }
        if view.superview !== container {
            view.frame = container.bounds
            view.autoresizingMask = [.width, .height]
            container.addSubview(view)
        }
    }
}
