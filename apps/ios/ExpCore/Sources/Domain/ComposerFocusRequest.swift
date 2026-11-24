import Foundation

/// #EXP-2004: when the Agent page spends the `new-chat` shortcut's pending
/// composer focus (app-shortcuts.json OPENING, cold start and warm alike).
/// On a cold start the composer mounts before the teams resolve and
/// is rebuilt for the team once it does: a focus spent on that first,
/// teamless composer went down with it. So the request waits for a composer
/// bound to the page's team, the one that stays.
public enum ComposerFocusRequest {
    public static func shouldSpend(
        pending: Bool,
        newChat: Bool,
        composerTeamId: String?,
        pageTeamId: String?
    ) -> Bool {
        pending && newChat && pageTeamId != nil && composerTeamId == pageTeamId
    }
}
