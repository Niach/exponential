import ExpCore
import ExpUI
import SwiftUI
import UIKit

/// EXP-1554, #EXP-2168 (header-actions.json `leading`): a pushed page's back
/// control is THE round header button (`HeaderCircleButton` ui-back, web
/// `HeaderBack`, Android `PageHeaderBack`), never the system's chevron. It
/// rides a leading item that opts out of the shared item glass (the face
/// draws its own circle); a pushed page header (`pageHeader(pushed: true)`,
/// `detailHeaderTitle`) hands its glyph and title up so they sit right after
/// the back, LEFT-aligned, in the SAME item. Hiding the system back control also disables UIKit's edge swipe,
/// so the modifier re-arms it.
extension View {
    func bareBackButton(enabled: Bool = true) -> some View {
        modifier(BareBackButton(enabled: enabled))
    }
}

/// A pushed page header's glyph and title, for the bare back item.
struct PushedTitleKey: PreferenceKey {
    struct Title: Equatable {
        let text: String
        var glyph: PageHeaderGlyphKind?
        /// `detailHeaderTitle`'s own view.
        var custom: Custom?
    }

    /// A custom title view, equal while its `key` is.
    struct Custom: Equatable {
        let key: String
        let view: AnyView

        static func == (lhs: Custom, rhs: Custom) -> Bool { lhs.key == rhs.key }
    }

    static var defaultValue: Title? { nil }
    static func reduce(value: inout Title?, nextValue: () -> Title?) {
        value = value ?? nextValue()
    }
}

private struct BareBackButton: ViewModifier {
    let enabled: Bool
    @Environment(\.dismiss) private var dismiss
    @State private var pushedTitle: PushedTitleKey.Title?

    func body(content: Content) -> some View {
        if enabled {
            content
                .onPreferenceChange(PushedTitleKey.self) { title in
                    MainActor.assumeIsolated { pushedTitle = title }
                }
                .navigationBarBackButtonHidden(true)
                .ownChromeToolbarItem(placement: .topBarLeading) {
                    HStack(spacing: CGFloat(HeaderActions.gap)) {
                        // The system control's id: UI tests find Back by it.
                        HeaderCircleButton(icon: AppIcons.uiBack, label: "Back", identifier: "BackButton") {
                            dismiss()
                        }
                        if let pushedTitle {
                            PageHeaderDetailTitle(title: pushedTitle)
                        }
                    }
                }
                .background(SwipeBackEnabler().frame(width: 0, height: 0))
        } else {
            content
        }
    }
}

/// Re-arms the edge swipe of the enclosing navigation controller (a hidden
/// system back control turns it off).
private struct SwipeBackEnabler: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> Probe { Probe() }
    func updateUIViewController(_ controller: Probe, context: Context) {}

    final class Probe: UIViewController {
        override func viewWillAppear(_ animated: Bool) {
            super.viewWillAppear(animated)
            arm()
        }

        override func viewDidAppear(_ animated: Bool) {
            super.viewDidAppear(animated)
            arm()
        }

        private func arm() {
            guard let nav = navigationController,
                  let pop = nav.interactivePopGestureRecognizer else { return }
            if SwipeBackDelegate.delegates.object(forKey: nav) == nil {
                SwipeBackDelegate.delegates.setObject(SwipeBackDelegate(nav: nav), forKey: nav)
            }
            pop.delegate = SwipeBackDelegate.delegates.object(forKey: nav)
            pop.isEnabled = true
        }
    }
}

@MainActor
private final class SwipeBackDelegate: NSObject, UIGestureRecognizerDelegate {
    static let delegates = NSMapTable<UINavigationController, SwipeBackDelegate>.weakToStrongObjects()

    weak var nav: UINavigationController?

    init(nav: UINavigationController) {
        self.nav = nav
    }

    nonisolated func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        MainActor.assumeIsolated {
            (nav?.viewControllers.count ?? 0) > 1
        }
    }
}
