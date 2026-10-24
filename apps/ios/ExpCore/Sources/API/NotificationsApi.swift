import Foundation

// Mirrors apps/web/src/lib/trpc/notifications.ts — inbox mark-read.
private struct NotificationIdInput: Encodable {
    let id: String
}

private struct NotificationIssueInput: Encodable {
    let issueId: String
}

private struct EmptyInput: Encodable {}

private struct EmptyResult: Decodable {}

public final class NotificationsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func markRead(accountId: String, id: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "notifications.markRead",
            input: NotificationIdInput(id: id)
        )
    }

    /// Opening an issue clears its inbox entries (EXP-92) — the read-on-open
    /// safety net for push taps and universal links that skip the inbox.
    public func markReadByIssue(accountId: String, issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "notifications.markReadByIssue",
            input: NotificationIssueInput(issueId: issueId)
        )
    }

    /// EXP-1373: `teamId` = only that team's rows (the phone inbox shows ONE
    /// active team); nil = every team (the input-less call).
    public func markAllRead(accountId: String, teamId: String? = nil) async throws {
        if let teamId {
            let _: EmptyResult = try await trpc.mutation(
                accountId: accountId,
                path: "notifications.markAllRead",
                input: MarkAllReadInput(teamId: teamId)
            )
        } else {
            let _: EmptyResult = try await trpc.mutation(
                accountId: accountId,
                path: "notifications.markAllRead",
                input: EmptyInput()
            )
        }
    }
}

/// `notifications.markAllRead({ teamId })`.
public struct MarkAllReadInput: Encodable, Equatable, Sendable {
    public let teamId: String

    public init(teamId: String) {
        self.teamId = teamId
    }
}
