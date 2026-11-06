import SwiftUI
import UIKit

// EXP-1676: THE overlay scrim ×4 behind a sheet (web Sheet overlay
// `bg-black/60 backdrop-blur-sm`, Android `GlassSheet`'s `ScrimColor` +
// `OverlayWindowBlur`, the alert's `GlassAlertHost`). A native `.sheet` only
// DIMS what it covers (and a partial-detent sheet on iOS 26+ not even that);
// this blurs and darkens it. The view sits in the presentation's container
// right under the sheet (over the system's dim) and stays tap-through: the
// dimming view under it keeps its tap-to-dismiss.

/// The scrim's numbers, pinned by `SheetBackdropTests`.
public enum SheetBackdropMetrics {
    /// Web's `bg-black/60`, laid over the blur. The blur's own dark tint
    /// stands in for the system's dim it covers (blurred under it, that dim
    /// washes out), so the black is the full scrim, not a remainder.
    public static let scrimOpacity = CGFloat(GlassAlertMetrics.scrimOpacity)
}

/// Installs the blurred scrim under the presenting sheet. Put it anywhere in
/// the sheet's content (GlassSheetChrome does); it draws nothing itself. A
/// child controller, so the sheet's own appearance transitions reach it: the
/// scrim fades in with the presentation and out with the dismissal (a drag
/// that springs back restores it).
struct SheetBackdropInstaller: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> InstallerController { InstallerController() }
    func updateUIViewController(_ controller: InstallerController, context: Context) {}

    final class InstallerController: UIViewController {
        private weak var backdrop: BackdropView?

        override func loadView() {
            let view = UIView()
            view.isUserInteractionEnabled = false
            view.backgroundColor = .clear
            self.view = view
        }

        override func viewWillAppear(_ animated: Bool) {
            super.viewWillAppear(animated)
            install()
        }

        override func viewDidAppear(_ animated: Bool) {
            super.viewDidAppear(animated)
            // A content change re-hosts the controller after the transition.
            install()
            backdrop?.alpha = 1
        }

        override func viewWillDisappear(_ animated: Bool) {
            super.viewWillDisappear(animated)
            guard let backdrop, sheet()?.isBeingDismissed == true else { return }
            fade(backdrop, to: 0, restoringOnCancel: 1)
        }

        /// The presented sheet: the top of this controller's parent chain.
        private func sheet() -> UIViewController? {
            var controller: UIViewController = self
            while let parent = controller.parent { controller = parent }
            return controller.presentingViewController != nil ? controller : nil
        }

        private func install() {
            guard backdrop?.superview == nil,
                  let sheet = sheet(),
                  // A sheet (SwiftUI's is a `_UIFormSheetPresentationController`,
                  // no UISheetPresentationController), never a full-screen cover.
                  ![.fullScreen, .overFullScreen].contains(sheet.modalPresentationStyle),
                  let container = sheet.presentationController?.containerView
            else { return }
            if let existing = container.subviews.compactMap({ $0 as? BackdropView }).first {
                backdrop = existing
                return
            }
            let view = BackdropView(scrim: SheetBackdropMetrics.scrimOpacity)
            view.frame = container.bounds
            view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            // Right under the sheet's own wrapper (`UIDropShadowView`), over
            // the system's dimming view: under it, the blur never showed.
            if let wrapper = container.subviews.first(where: { sheet.view.isDescendant(of: $0) }) {
                container.insertSubview(view, belowSubview: wrapper)
            } else {
                container.insertSubview(view, at: 0)
            }
            backdrop = view
            if sheet.isBeingPresented {
                view.alpha = 0
                fade(view, to: 1, restoringOnCancel: 0)
            }
        }

        private func fade(_ view: UIView, to alpha: CGFloat, restoringOnCancel previous: CGFloat) {
            guard let coordinator = sheet()?.transitionCoordinator ?? transitionCoordinator else {
                view.alpha = alpha
                return
            }
            coordinator.animate(alongsideTransition: { _ in view.alpha = alpha }) { context in
                if context.isCancelled { view.alpha = previous }
            }
        }
    }

    /// The blur + web's black over it.
    final class BackdropView: UIView {
        init(scrim: CGFloat) {
            super.init(frame: .zero)
            isUserInteractionEnabled = false
            let blur = UIVisualEffectView(effect: UIBlurEffect(style: .dark))
            blur.frame = bounds
            blur.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            addSubview(blur)
            let black = UIView(frame: bounds)
            black.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            black.backgroundColor = UIColor.black.withAlphaComponent(scrim)
            addSubview(black)
        }

        required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    }
}
