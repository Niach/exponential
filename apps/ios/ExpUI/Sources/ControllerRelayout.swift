import UIKit

extension UIViewController {
    /// This controller and every child below it, depth first (a navigation
    /// controller's stacked pages are its children).
    public var controllerTree: [UIViewController] {
        [self] + children.flatMap(\.controllerTree)
    }

    /// Marks every loaded controller view in the tree for layout, so each one
    /// reads its safe area again (`TopBannerStack`: a banner moved the stack).
    public func relayoutControllerTree() {
        for controller in controllerTree { controller.viewIfLoaded?.setNeedsLayout() }
    }

    /// `relayoutControllerTree` from every window's root.
    @MainActor
    public static func relayoutWindowControllers() {
        for scene in UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }) {
            for window in scene.windows { window.rootViewController?.relayoutControllerTree() }
        }
    }
}
