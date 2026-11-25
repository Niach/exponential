import ExpCore
import Foundation
import GRDB

@Observable
final class DeepLinkBus: @unchecked Sendable {
    var pendingIssueId: String?
    // Server user id the issue link targets (set by push taps — the payload
    // carries the recipient's user id). Lets the navigator open the issue
    // under the signed-in account it belongs to on multi-account devices;
    // nil for plain URL links, which stay active-account.
    var pendingIssueUserId: String?
    // Local account id the issue resolved under (set by universal links —
    // EXP-92 — where the account is known from the URL-host match, not a
    // userId). Wins over the userId mapping when set.
    var pendingIssueAccountId: String?
    // A Stack card member tap: SWAP the issue on top of the stack instead of
    // pushing (web + desktop parity), so Back never walks the stack.
    var pendingIssueReplacesTop = false
    // EXP-1241: the comment the issue opens scrolled to (and briefly
    // highlights) — a run's comment card tap (web `?comment=<id>`).
    var pendingIssueCommentId: String?
    var pendingInviteToken: String?
    // An agent_message push tap (EXP-801): the row lives in the My Work
    // inbox and nowhere else, so the tap lands there. Carries the recipient's
    // server user id like the other push kinds.
    var pendingInbox = false
    var pendingInboxUserId: String?
    // EXP-1373: the row's team when the payload names one — the inbox shows
    // the active team's rows only, so the tap switches to it first.
    var pendingInboxTeamId: String?
    // A push tap naming a session (EXP-980 wall, #EXP-1456 any type): the
    // navigator opens it by `InboxRoute.sessionTarget`.
    // Carries the recipient's server user id like the other push kinds, so a
    // multi-account device opens it under the account it was delivered to.
    var pendingSessionId: String?
    var pendingSessionUserId: String?
    /// waiting-cards.json DEEP LINKS: the card the session push is about.
    var pendingSessionCard: NotificationRouting.Card?
    // #EXP-1674: where a link resolved to (`LinkOpener`), under its account.
    var pendingLinkLanding: LinkLanding?
    // #EXP-1534 (inbox-route.json): an `agent_signed_out` inbox row or push
    // tap — the devices ROW id whose settings open (the Devices page with
    // that device's settings sheet), under the account it belongs to: the
    // inbox row knows it (`pendingDeviceAccountId`), a push only the
    // recipient's server user id.
    var pendingDeviceRowId: String?
    var pendingDeviceUserId: String?
    var pendingDeviceAccountId: String?
    // #EXP-1534: the device row the Devices page (`AgentsView`) opens the
    // settings sheet for once its list holds it; set by the navigator as it
    // lands there, cleared by the page.
    var pendingDeviceSettingsRowId: String?
    // A web URL the app cannot render (another origin, a web-only page).
    // MainNavigator presents it in an in-app Safari sheet —
    // NEVER hand it back to UIApplication.open: the app is entitled for the
    // link and would re-open itself in a loop.
    var pendingExternalUrl: URL?
    // #EXP-1638 / #EXP-1830 (app-shortcuts.json OPENING): a long-press menu
    // pick on the app icon, a home widget tap or an App Intent, under the
    // ACTIVE account (the menu and the widgets follow it). The navigator
    // drops back to the Agent tab root, then pushes it.
    var pendingLaunch: LaunchTarget?
    // #EXP-1830: the Board tab opens on its My issues chip (a My issues
    // widget's header); cleared by `IssuesHomeView`.
    var pendingMyIssues = false
    // #EXP-1638: the Agent tab's composer takes focus once it is up (the
    // `new-chat` shortcut); cleared by `AgentPageView`.
    var pendingComposerFocus = false

    func navigateToIssue(_ issueId: String, userId: String? = nil) {
        pendingIssueUserId = userId
        pendingIssueAccountId = nil
        pendingIssueId = issueId
    }

    func navigateToIssue(
        _ issueId: String, accountId: String, replacingTop: Bool = false, commentId: String? = nil
    ) {
        pendingIssueUserId = nil
        pendingIssueAccountId = accountId
        pendingIssueReplacesTop = replacingTop
        pendingIssueCommentId = commentId
        pendingIssueId = issueId
    }

    func navigateToInvite(_ token: String) {
        pendingInviteToken = token
    }

    func navigateToInbox(userId: String? = nil, teamId: String? = nil) {
        pendingInboxUserId = userId
        pendingInboxTeamId = teamId
        pendingInbox = true
    }

    func navigateToSession(_ sessionId: String, userId: String? = nil, card: NotificationRouting.Card? = nil) {
        pendingSessionUserId = userId
        pendingSessionCard = card
        pendingSessionId = sessionId
    }

    func navigateToDevice(_ deviceRowId: String, userId: String? = nil, accountId: String? = nil) {
        pendingDeviceUserId = userId
        pendingDeviceAccountId = accountId
        pendingDeviceRowId = deviceRowId
    }

    func consumeDevice() -> String? {
        let id = pendingDeviceRowId
        pendingDeviceRowId = nil
        pendingDeviceUserId = nil
        pendingDeviceAccountId = nil
        return id
    }

    func navigateToLink(_ landing: LinkLanding) {
        pendingLinkLanding = landing
    }

    func consumeLinkLanding() -> LinkLanding? {
        let landing = pendingLinkLanding
        pendingLinkLanding = nil
        return landing
    }

    func openExternal(_ url: URL) {
        pendingExternalUrl = url
    }

    func consume() -> String? {
        let id = pendingIssueId
        pendingIssueId = nil
        pendingIssueUserId = nil
        pendingIssueAccountId = nil
        pendingIssueReplacesTop = false
        pendingIssueCommentId = nil
        return id
    }

    func navigateToLaunch(_ target: LaunchTarget) {
        pendingLaunch = target
    }

    func consumeLaunch() -> LaunchTarget? {
        let target = pendingLaunch
        pendingLaunch = nil
        return target
    }

    func consumeExternalUrl() -> URL? {
        let url = pendingExternalUrl
        pendingExternalUrl = nil
        return url
    }

    func consumeInvite() -> String? {
        let token = pendingInviteToken
        pendingInviteToken = nil
        return token
    }

    func consumeSession() -> String? {
        let id = pendingSessionId
        pendingSessionId = nil
        pendingSessionUserId = nil
        pendingSessionCard = nil
        return id
    }

    /// Returns the recipient's user id (nil when none) when an inbox tap is
    /// pending; `nil` with `pending == false` otherwise.
    func consumeInbox() -> (pending: Bool, userId: String?, teamId: String?) {
        let result = (pending: pendingInbox, userId: pendingInboxUserId, teamId: pendingInboxTeamId)
        pendingInbox = false
        pendingInboxUserId = nil
        pendingInboxTeamId = nil
        return result
    }
}

/// #EXP-1830 (app-shortcuts.json OPENING, home-widgets.json TAPS): what a
/// launcher surface opens: an app shortcut, a widget's header or row, an
/// App Intent. Every one lands with THE back stack rule (the Agent tab root
/// under it), cold or warm.
enum LaunchTarget: Equatable {
    case shortcut(AppShortcuts.Target)
    /// THE start screen (a signed-out widget).
    case home
    case issue(String)
    case board(String)
    /// The Board tab's My issues.
    case myIssues
    /// THE Needs you page.
    case needsYou
    /// A needs-you row: its root opened by `opens` (needs-you.json) on
    /// `messageId`.
    case needsYouRow(rootId: String, messageId: String, opens: NeedsYou.Opens)

    /// The widgets' `exponential://` links (`HomeWidgets.Links`); nil =
    /// not a launcher link.
    init?(url: URL) {
        guard url.scheme == HomeWidgets.Links.scheme, let host = url.host else { return nil }
        let first: String? = url.pathComponents.dropFirst().first
        let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        func item(_ name: String) -> String? { query.first { $0.name == name }?.value }
        switch host {
        case "home": self = .home
        case "my-issues": self = .myIssues
        case "needs-you": self = .needsYou
        case "board":
            guard let first, !first.isEmpty else { return nil }
            self = .board(first)
        case "issue":
            guard let first, !first.isEmpty else { return nil }
            self = .issue(first)
        case "session":
            guard let first, !first.isEmpty, let opens = item("opens").flatMap(NeedsYou.Opens.init(rawValue:)),
                  let messageId = item("message") else { return nil }
            self = .needsYouRow(rootId: first, messageId: messageId, opens: opens)
        default: return nil
        }
    }
}
