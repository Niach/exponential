import Foundation

/// #EXP-2004: when the Agent page spends the `new-chat` shortcut's pending
/// composer focus (app-shortcuts.json OPENING, cold start and warm alike).
/// On a cold start the composer mounts before the active team resolves and
/// is rebuilt for the team once it does: a focus spent on that first,
/// teamless composer went down with it. So the request waits for a composer
/// bound to the page's team, the one that stays.
public enum ComposerFocusRequest {
    public static func shouldSpend(
        pending: Bool,
        isTabRoot: Bool,
        composerTeamId: String?,
        pageTeamId: String?
    ) -> Bool {
        pending && isTabRoot && pageTeamId != nil && composerTeamId == pageTeamId
    }
}
