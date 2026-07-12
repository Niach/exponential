import Foundation

@Observable
final class DeepLinkBus: @unchecked Sendable {
    var pendingIssueId: String?
    // Server user id the issue link targets (set by push taps — the payload
    // carries the recipient's user id). Lets the navigator open the issue
    // under the signed-in account it belongs to on multi-account devices;
    // nil for plain URL links, which stay active-account.
    var pendingIssueUserId: String?
    var pendingInviteToken: String?

    func navigateToIssue(_ issueId: String, userId: String? = nil) {
        pendingIssueUserId = userId
        pendingIssueId = issueId
    }

    func navigateToInvite(_ token: String) {
        pendingInviteToken = token
    }

    func consume() -> String? {
        let id = pendingIssueId
        pendingIssueId = nil
        pendingIssueUserId = nil
        return id
    }

    func consumeInvite() -> String? {
        let token = pendingInviteToken
        pendingInviteToken = nil
        return token
    }
}
