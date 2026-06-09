import Foundation

@Observable
final class DeepLinkBus: @unchecked Sendable {
    var pendingIssueId: String?
    var pendingInviteToken: String?

    func navigateToIssue(_ issueId: String) {
        pendingIssueId = issueId
    }

    func navigateToInvite(_ token: String) {
        pendingInviteToken = token
    }

    func consume() -> String? {
        let id = pendingIssueId
        pendingIssueId = nil
        return id
    }

    func consumeInvite() -> String? {
        let token = pendingInviteToken
        pendingInviteToken = nil
        return token
    }
}
