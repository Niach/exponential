import SwiftUI

/// EXP-1212: the navigator's hold on a New issue page WITH content.
///
/// The draft page registers an interceptor while it is on screen; every
/// navigator-level path change (a tab, a pushed route, a deep link or push
/// tap) asks it first while that draft is the top route. A held change is
/// handed to the page, which asks `IssueDraftPage.Leave` and continues it
/// (Create · Keep as draft · Discard) or drops it (the dialog dismissed).
///
/// The same reference-box shape as `PushRouteAction`: ONE instance in the
/// navigator's `@State`, handed to the environment on every body pass.
final class IssueDraftLeaveGuard: @unchecked Sendable {
    /// A navigation the page holds: `proceed` continues it, `dropped` runs
    /// when the user stays instead.
    struct Held {
        let proceed: () -> Void
        let dropped: () -> Void
    }

    /// The draft the interceptor belongs to, and the interceptor: true =
    /// HELD (the page will continue or drop it), false = go now.
    private var draftId: String?
    private var interceptor: ((Held) -> Bool)?

    init() {}

    func register(draftId: String, interceptor: @escaping (Held) -> Bool) {
        self.draftId = draftId
        self.interceptor = interceptor
    }

    /// Only the page that registered may clear it: a draft pushed over another
    /// one disappears AFTER the new one appeared.
    func unregister(draftId: String) {
        guard self.draftId == draftId else { return }
        self.draftId = nil
        interceptor = nil
    }

    /// Whether the draft on top (`topDraftId`) holds this navigation. False
    /// = nothing registered for it, or it has no content: go now.
    func holds(topDraftId: String, _ held: Held) -> Bool {
        guard topDraftId == draftId, let interceptor else { return false }
        return interceptor(held)
    }
}

private struct IssueDraftLeaveGuardKey: EnvironmentKey {
    /// No navigator above: nothing is ever held.
    static let defaultValue = IssueDraftLeaveGuard()
}

extension EnvironmentValues {
    /// The enclosing `MainNavigator`'s draft hold (EXP-1212).
    var issueDraftLeaveGuard: IssueDraftLeaveGuard {
        get { self[IssueDraftLeaveGuardKey.self] }
        set { self[IssueDraftLeaveGuardKey.self] = newValue }
    }
}
