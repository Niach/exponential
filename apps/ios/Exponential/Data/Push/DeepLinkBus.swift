import Foundation

@Observable
final class DeepLinkBus: @unchecked Sendable {
    var pendingIssueId: String?

    func navigateToIssue(_ issueId: String) {
        pendingIssueId = issueId
    }

    func consume() -> String? {
        let id = pendingIssueId
        pendingIssueId = nil
        return id
    }
}
