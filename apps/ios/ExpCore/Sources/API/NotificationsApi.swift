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

    /// #EXP-1815: the member's notification prefs (the source switches).
    public func emailPrefs(accountId: String) async throws -> NotificationEmailPrefs {
        try await trpc.query(accountId: accountId, path: "notifications.emailPrefs")
    }

    /// #EXP-1815: writes the WHOLE source-switch object (after `toggleSourcePref`).
    public func updateSourcePrefs(accountId: String, _ prefs: NotificationSourcePrefs) async throws {
        let _: NotificationEmailPrefs = try await trpc.mutation(
            accountId: accountId,
            path: "notifications.updateEmailPrefs",
            input: UpdateSourcePrefsInput(sourcePrefs: prefs)
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

/// #EXP-1815 (notification-delivery.json SOURCE SWITCHES): the session-source
/// switches `{assistant?, temporary?, agents?: {<agentId>: bool}}`, absent = on
/// (the wire twin of ExpPush `NotificationDelivery.SourcePrefs`; ExpCore never
/// links ExpPush).
public struct NotificationSourcePrefs: Codable, Equatable, Sendable {
    public var assistant: Bool?
    public var temporary: Bool?
    public var agents: [String: Bool]?

    public init(assistant: Bool? = nil, temporary: Bool? = nil, agents: [String: Bool]? = nil) {
        self.assistant = assistant
        self.temporary = temporary
        self.agents = agents
    }
}

/// The part of `notifications.emailPrefs` the phones read.
public struct NotificationEmailPrefs: Decodable, Equatable, Sendable {
    public let sourcePrefs: NotificationSourcePrefs?
}

/// `notifications.updateEmailPrefs({ sourcePrefs })`: the WHOLE object.
public struct UpdateSourcePrefsInput: Encodable, Equatable, Sendable {
    public let sourcePrefs: NotificationSourcePrefs
}

/// `notifications.markAllRead({ teamId })`.
public struct MarkAllReadInput: Encodable, Equatable, Sendable {
    public let teamId: String

    public init(teamId: String) {
        self.teamId = teamId
    }
}
