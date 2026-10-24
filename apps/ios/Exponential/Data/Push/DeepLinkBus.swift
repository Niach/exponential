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
    // A session_blocked push tap (EXP-980): the session that hit a rate limit.
    // Carries the recipient's server user id like the other push kinds, so a
    // multi-account device opens it under the account it was delivered to.
    var pendingSessionId: String?
    var pendingSessionUserId: String?
    // EXP-1284: a tapped thread link (EXP-1291: a root session's id), under
    // the account it belongs to.
    var pendingThreadId: String?
    var pendingThreadAccountId: String?
    // The page the thread opens on (a PR body's `?view=guide` → `.guide`);
    // nil = its Conversation.
    var pendingThreadPage: ThreadPage?
    // EXP-825: a `/t/{team}/agent` universal link — Home in that team,
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

    func navigateToSession(_ sessionId: String, userId: String? = nil) {
        pendingSessionUserId = userId
        pendingSessionId = sessionId
    }

    func navigateToThread(_ threadId: String, accountId: String, page: ThreadPage? = nil) {
        pendingThreadAccountId = accountId
        pendingThreadPage = page
        pendingThreadId = threadId
    }

    /// EXP-1291: a `/t/{team}/sessions/{id}` link (the PR body footer, agent
    /// prose). A ROOT opens its thread — on the Guide with `?view=guide` —
    /// and so does an id this account has not synced (the thread screen
    /// resolves or says not found); a CHILD opens its raw session.
    func navigateToSessionLink(
        _ sessionId: String, account: ServerAccount, guide: Bool, db: DatabaseManager
    ) {
        let row = (try? db.pool(forAccountId: account.id).read { db in
            try SessionQueries.session(db: db, id: sessionId)
        }) ?? nil
        if let row, !row.isRoot {
            navigateToSession(sessionId, userId: account.userId)
        } else {
            navigateToThread(sessionId, accountId: account.id, page: guide ? .guide : nil)
        }
    }

    func consumeThread() -> (threadId: String, accountId: String, page: ThreadPage?)? {
        defer {
            pendingThreadId = nil
            pendingThreadAccountId = nil
            pendingThreadPage = nil
        }
        guard let threadId = pendingThreadId else { return nil }
        return (threadId, pendingThreadAccountId ?? "", pendingThreadPage)
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
        pendingIssueReplacesTop = false
        pendingIssueCommentId = nil
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

    func consumeSession() -> String? {
        let id = pendingSessionId
        pendingSessionId = nil
        pendingSessionUserId = nil
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
    func consumeInbox() -> (pending: Bool, userId: String?, teamId: String?) {
        let result = (pending: pendingInbox, userId: pendingInboxUserId, teamId: pendingInboxTeamId)
        pendingInbox = false
        pendingInboxUserId = nil
        pendingInboxTeamId = nil
        return result
    }
}
