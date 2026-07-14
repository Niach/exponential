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
}
