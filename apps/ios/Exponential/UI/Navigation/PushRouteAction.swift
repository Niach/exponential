import SwiftUI

/// Pushes an `AppRoute` onto the navigator's path from anywhere below it
/// (EXP-698 r5).
///
/// It exists because the issue list's rows stopped being `NavigationLink`s: a
/// link inside a `List` row draws the system disclosure chevron at the row's
/// edge, OUTSIDE the glass card, which is exactly the chevron the other three
/// clients draw INSIDE it. A plain `Button` keeps the card intact — but then
/// the row needs a way to navigate, and the typed `path` lives in
/// `MainNavigator`.
///
/// Reference box around the closure, the same shape `GlassMenuDismissAction`
/// (and SwiftUI's own `DismissAction`) has: a bare `() -> Void` cannot be an
/// `EnvironmentKey.defaultValue` under strict concurrency, and the closure
/// captures view state that is main-actor bound by construction.
/// The handler is a `var` set after construction, so the navigator can hold
/// ONE instance in `@State` and hand the same object to the environment on
/// every body pass. Building a fresh box per pass would invalidate every
/// reader of `\.pushRoute` on every render — the environment compares
/// reference values by identity.
final class PushRouteAction: @unchecked Sendable {
    private var handler: ((AppRoute) -> Void)?

    init() {}

    init(_ handler: @escaping (AppRoute) -> Void) {
        self.handler = handler
    }

    /// Point the box at the navigator's path. Called from the navigator's
    /// `onAppear`, before any row below it can be tapped.
    func setHandler(_ handler: @escaping (AppRoute) -> Void) {
        self.handler = handler
    }

    /// A no-op until the handler is set (a preview, a detached host) rather
    /// than a crash.
    func callAsFunction(_ route: AppRoute) {
        handler?(route)
    }
}

private struct PushRouteKey: EnvironmentKey {
    /// No navigator above: pushing goes nowhere.
    static let defaultValue = PushRouteAction()
}

extension EnvironmentValues {
    /// Appends a route to the enclosing `MainNavigator`'s path.
    var pushRoute: PushRouteAction {
        get { self[PushRouteKey.self] }
        set { self[PushRouteKey.self] = newValue }
    }
}
