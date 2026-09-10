import Foundation

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
    var pendingInviteToken: String?
    // A support_reply push tap (EXP-180): the ticket to open in the Support
    // thread view. Carries the recipient's server user id like issue pushes so
    // multi-account devices open it under the right account.
    var pendingSupportThreadId: String?
    var pendingSupportThreadUserId: String?
    // An agent_message push tap (EXP-801): the row lives in the My Work
    // inbox and nowhere else, so the tap lands there. Carries the recipient's
    // server user id like the other push kinds.
    var pendingInbox = false
    var pendingInboxUserId: String?
    // EXP-825: a `/t/{team}/agent` universal link — the team's Agent page,
    // under the signed-in account whose host matched the URL.
    var pendingAgentTeamSlug: String?
    var pendingAgentAccountId: String?
    // A web URL the app was opened with but cannot render (unknown host, issue
    // not synced/visible). MainNavigator presents it in an in-app Safari sheet —
    // NEVER hand it back to UIApplication.open: the app is entitled for the
    // link and would re-open itself in a loop.
    var pendingExternalUrl: URL?

    func navigateToIssue(_ issueId: String, userId: String? = nil) {
        pendingIssueUserId = userId
        pendingIssueAccountId = nil
        pendingIssueId = issueId
    }

    func navigateToIssue(_ issueId: String, accountId: String) {
        pendingIssueUserId = nil
        pendingIssueAccountId = accountId
        pendingIssueId = issueId
    }

    func navigateToInvite(_ token: String) {
        pendingInviteToken = token
    }

    func navigateToSupportThread(_ threadId: String, userId: String? = nil) {
        pendingSupportThreadUserId = userId
        pendingSupportThreadId = threadId
    }

    func navigateToInbox(userId: String? = nil) {
        pendingInboxUserId = userId
        pendingInbox = true
    }

    func navigateToAgent(teamSlug: String, accountId: String) {
        pendingAgentAccountId = accountId
        pendingAgentTeamSlug = teamSlug
    }

    func openExternal(_ url: URL) {
        pendingExternalUrl = url
    }

    func consume() -> String? {
        let id = pendingIssueId
        pendingIssueId = nil
        pendingIssueUserId = nil
        pendingIssueAccountId = nil
        return id
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

    func consumeSupportThread() -> String? {
        let id = pendingSupportThreadId
        pendingSupportThreadId = nil
        pendingSupportThreadUserId = nil
        return id
    }

    func consumeAgent() -> (teamSlug: String, accountId: String)? {
        defer {
            pendingAgentTeamSlug = nil
            pendingAgentAccountId = nil
        }
        guard let slug = pendingAgentTeamSlug, let accountId = pendingAgentAccountId else { return nil }
        return (slug, accountId)
    }

    /// Returns the recipient's user id (nil when none) when an inbox tap is
    /// pending; `nil` with `pending == false` otherwise.
    func consumeInbox() -> (pending: Bool, userId: String?) {
        let result = (pending: pendingInbox, userId: pendingInboxUserId)
        pendingInbox = false
        pendingInboxUserId = nil
        return result
    }
}
