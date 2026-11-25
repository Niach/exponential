import ExpUI
import SwiftUI
import UIKit

/// EXP-1554: a pushed page's back control is the BARE `ui-back` arrow
/// (web-mobile, Android), never the system's Liquid Glass circle. The system
/// chevron stands down and the arrow rides a leading item that opts out of
/// the shared item glass; a pushed page header (`pageHeader(pushed: true)`)
/// hands its large title up so it sits right after the arrow in the SAME
/// item. Hiding the system back control also disables UIKit's edge swipe,
/// so the modifier re-arms it.
extension View {
    func bareBackButton(enabled: Bool = true) -> some View {
        modifier(BareBackButton(enabled: enabled))
    }
}

/// A pushed page header's large title, for the bare back item.
struct PushedLargeTitleKey: PreferenceKey {
    static let defaultValue: String? = nil
    static func reduce(value: inout String?, nextValue: () -> String?) {
        value = value ?? nextValue()
    }
}

private struct BareBackButton: ViewModifier {
    let enabled: Bool
    @Environment(\.dismiss) private var dismiss
    @State private var pushedTitle: String?

    func body(content: Content) -> some View {
        if enabled {
            content
                .onPreferenceChange(PushedLargeTitleKey.self) { title in
                    MainActor.assumeIsolated { pushedTitle = title }
                }
                .navigationBarBackButtonHidden(true)
                .ownChromeToolbarItem(placement: .topBarLeading) {
                    HStack(spacing: 4) {
                        Button {
                            dismiss()
                        } label: {
                            AppIcon(AppIcons.uiBack, size: AppIcon.Size.medium, weight: .medium)
                                .foregroundStyle(.white)
                                .frame(width: 32, height: 32)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Back")
                        // The system control's id: UI tests find Back by it.
                        .accessibilityIdentifier("BackButton")
                        if let pushedTitle {
                            Text(pushedTitle)
                                .font(.system(size: 34, weight: .bold))
                                .foregroundStyle(.white)
                                .lineLimit(1)
                                .fixedSize()
                                .accessibilityAddTraits(.isHeader)
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
