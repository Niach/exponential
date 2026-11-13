import Foundation

// Mirrors apps/web/src/lib/trpc/sessions.ts ( EXP-1291). A chat
// IS a root session, children are sub-sessions, and (NEW-2) a team agent is
// a definition (`AgentsApi`) whose conversations are roots with `agent_id` —
// and ONE entrypoint starts and messages them all: `sessions.send`. The database is the bus: a new session is a `queued` row
// naming its device, a message a `messages` row; both land here over the
// shapes. Every write answers with a `txid` the client never needs, so it is
// not decoded. The `pullRequests` router (merge/close/files) lives at the
// bottom: PR state left `issues` for the synced `pull_requests` rows.

/// `sessions.stop`'s answer (chats.json STOP): `delivered` false = the relay
/// reached no device for a live, non-queued session (toast
/// `Chats.Copy.Toasts.stopNotDelivered`); nil = an older server that does not
/// say. Decoded leniently: any shape reads, a missing / non-bool field is nil.
public struct SessionsStopResult: Decodable, Sendable, Equatable {
    public let delivered: Bool?

    public init(delivered: Bool?) {
        self.delivered = delivered
    }

    private enum CodingKeys: String, CodingKey { case delivered }

    public init(from decoder: Decoder) throws {
        let container = try? decoder.container(keyedBy: CodingKeys.self)
        delivered = (try? container?.decodeIfPresent(Bool.self, forKey: .delivered)) ?? nil
    }
}

// MARK: - Other inputs

/// #EXP-1641: a write into sealed content this install could not seal (no
/// key, no crypto, the card not here): nothing was sent.
public struct SealedOnlyError: Error, Equatable, Sendable {
    public init() {}
}

/// A plan card's decision (`sessions.answer({decision})`).
public enum PlanDecision: String, Encodable, Sendable {
    case approved, rejected
}

/// `sessions.answer`: a `question` card takes `answer`, a `plan` card a
/// `decision` (+ optional `feedback`; `fresh` = the approval starts in a
/// fresh context, ask-dock.json `planOptions`, sent only with `approved`).
/// Nil fields are omitted. EXP-1280 (plan-card.json): `answers` = a plan's
/// picked labels by question id, or a question's picked labels;
/// `surfaceData` = the inline surface's data model at answer time.
struct AnswerMessageInput: Encodable {
    let messageId: String
    let answer: String?
    let decision: PlanDecision?
    let fresh: Bool?
    let feedback: String?
    var answers: CardAnswers? = nil
    var surfaceData: CardJSON? = nil
}

/// #EXP-1561 (waiting-cards.json DISMISS): `sessions.answer({messageId,
/// dismissed: true})`, nothing else rides along (a sealed card too: the
/// server stamps the clear fields beside its envelope).
struct DismissCardInput: Encodable {
    let messageId: String
    let dismissed = true
}

/// `sessions.answer`'s `answers`: a plan's labels by question id, or a
/// question card's picked labels.
public enum CardAnswers: Encodable, Sendable, Equatable {
    case plan([String: [String]])
    case question([String])

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case let .plan(answers): try container.encode(answers)
        case let .question(labels): try container.encode(labels)
        }
    }
}

extension CardJSON: Encodable {
    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case let .bool(value): try container.encode(value)
        case let .number(value): try container.encode(value)
        case let .string(value): try container.encode(value)
        case let .array(values): try container.encode(values)
        case let .object(values): try container.encode(values)
        }
    }
}

private struct McpPicksResult: Decodable {
    let mcpServerIds: [String]
}

private struct SessionIdInput: Encodable {
    let sessionId: String
}

private struct SetReadInput: Encodable {
    let sessionIds: [String]
    let read: Bool
}

private struct SetArchivedInput: Encodable {
    let sessionIds: [String]
    let archived: Bool
}

private struct LinkIssuesInput: Encodable {
    let sessionId: String
    let issueIds: [String]
}

private struct UnlinkIssueInput: Encodable {
    let sessionId: String
    let issueId: String
}

/// EXP-1334: a live session's mid-run picks (`sessions.configure`); nil
/// fields stay off the wire.
public struct SessionConfigureInput: Encodable, Sendable, Equatable {
    public let sessionId: String
    public var effort: String?
    public var ultracode: Bool?
    public var computerUse: Bool?
    public var mcpServerIds: [String]?

    public init(
        sessionId: String,
        effort: String? = nil,
        ultracode: Bool? = nil,
        computerUse: Bool? = nil,
        mcpServerIds: [String]? = nil
    ) {
        self.sessionId = sessionId
        self.effort = effort
        self.ultracode = ultracode
        self.computerUse = computerUse
        self.mcpServerIds = mcpServerIds
    }
}

private struct MessageIdInput: Encodable {
    let messageId: String
}

/// EXP-1323: where a suggested task starts — a new child, or `here` (sent
/// to the session that posted the card).
public enum SuggestionStartWhere: String, Encodable, Sendable {
    case child, here
}

/// Internal (not private) so the wire test can pin the omitted fields.
struct StartSuggestionInput: Encodable {
    let messageId: String
    let itemId: String
    let `where`: SuggestionStartWhere
    let deviceId: String?
    let account: String?
    let model: String?

    enum CodingKeys: String, CodingKey {
        case messageId, itemId, `where`, deviceId, account, model
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(messageId, forKey: .messageId)
        try c.encode(itemId, forKey: .itemId)
        try c.encode(`where`, forKey: .where)
        try c.encodeIfPresent(deviceId, forKey: .deviceId)
        try c.encodeIfPresent(account, forKey: .account)
        try c.encodeIfPresent(model, forKey: .model)
    }
}

private struct SuggestionItemInput: Encodable {
    let messageId: String
    let itemId: String
}

// MARK: - API

public final class SessionsApi: Sendable {
    private let trpc: TrpcClient

    /// EXP-1459: seals what this install writes (set once at launch).
    public let sealing = SessionSealerSlot()
    private var sealer: SessionSealer? { sealing.sealer }

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// THE entrypoint: start a chat or a child, or message a session or an
    /// agent. A refusal throws `TrpcError.httpError`, whose description is
    /// the server's message.
    /// #FEED-237: `clientMessageId` = the outbox entry's clientId (F21: a
    /// resend naming it returns the row the server already stored).
    @discardableResult
    public func send(accountId: String, _ input: SessionsSendInput, clientMessageId: String? = nil) async throws -> SessionsSendResult {
        // EXP-1459: the text leaves sealed to the tree's key (+ mentions).
        guard let sealer else {
            guard let clientMessageId else {
                return try await trpc.mutation(accountId: accountId, path: "sessions.send", input: input)
            }
            return try await trpc.mutation(
                accountId: accountId, path: "sessions.send",
                input: SealedSendInput(input: input, clientMessageId: clientMessageId)
            )
        }
        var prepared = await sealer.prepare(accountId: accountId, input)
        if let clientMessageId { prepared.input.clientMessageId = clientMessageId }
        let result: SessionsSendResult = try await trpc.mutation(accountId: accountId, path: "sessions.send", input: prepared.input)
        await sealer.completed(prepared, result: result)
        return result
    }

    /// #EXP-1641: a message into an ENCRYPTED tree (`rootId`) that never
    /// leaves in clear: throws `SealedOnlyError` when it cannot be sealed
    /// here, nothing sent.
    @discardableResult
    public func sendSealed(accountId: String, _ input: SessionsSendInput, rootId: String) async throws -> SessionsSendResult {
        guard let prepared = await sealer?.prepareSealed(accountId: accountId, input, rootId: rootId) else {
            throw SealedOnlyError()
        }
        let result: SessionsSendResult = try await trpc.mutation(accountId: accountId, path: "sessions.send", input: prepared.input)
        await sealer?.completed(prepared, result: result)
        return result
    }

    /// Interrupt the session's current turn (it stays live, goes idle): the
    /// TURN interrupt (composer stop, agent menu Stop). `delivered == false`
    /// → toast `Chats.Copy.Toasts.stopNotDelivered`.
    @discardableResult
    public func stop(accountId: String, sessionId: String) async throws -> SessionsStopResult {
        try await trpc.mutation(
            accountId: accountId, path: "sessions.stop", input: SessionIdInput(sessionId: sessionId)
        )
    }

    /// Session 58cb32df (chats.json): delete a ROOT chat (not an agent
    /// conversation, not an app session); the server ends what is live, then
    /// deletes the tree and its messages. FORBIDDEN / NOT_FOUND / BAD_REQUEST
    /// throw like any refusal.
    public func delete(accountId: String, sessionId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.delete", input: SessionIdInput(sessionId: sessionId)
        )
    }

    /// End a session explicitly; ending a ROOT ends its whole tree. Agents
    /// refuse.
    public func end(accountId: String, sessionId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.end", input: SessionIdInput(sessionId: sessionId)
        )
    }

    /// Link issues to a chat (always lands on the ROOT; the header "+").
    public func linkIssues(accountId: String, sessionId: String, issueIds: [String]) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "sessions.linkIssues",
            input: LinkIssuesInput(sessionId: sessionId, issueIds: issueIds)
        )
    }

    /// EXP-1334: change a running session's Plan / Effort / Ultracode /
    /// Computer use / MCP servers (the composer's "+" and options line).
    public func configure(accountId: String, _ input: SessionConfigureInput) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "sessions.configure", input: input)
    }

    /// EXP-1334: a session's MCP pick (server-only; whoever may steer reads it).
    public func mcpPicks(accountId: String, sessionId: String) async throws -> [String] {
        let result: McpPicksResult = try await trpc.query(
            accountId: accountId, path: "sessions.mcpPicks", input: SessionIdInput(sessionId: sessionId)
        )
        return result.mcpServerIds
    }

    public func unlinkIssue(accountId: String, sessionId: String, issueId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "sessions.unlinkIssue",
            input: UnlinkIssueInput(sessionId: sessionId, issueId: issueId)
        )
    }

    /// Answer a `question` card (`answer` / `answers`) or decide a `plan`
    /// card (`decision` + optional `answers`, `feedback`) IN the card;
    /// `surfaceData` = the inline surface's data model (EXP-1280).
    public func answer(
        accountId: String,
        messageId: String,
        answer: String? = nil,
        decision: PlanDecision? = nil,
        fresh: Bool = false,
        feedback: String? = nil,
        answers: CardAnswers? = nil,
        surfaceData: CardJSON? = nil,
        sealedOnly: Bool = false
    ) async throws {
        let input = AnswerMessageInput(
            messageId: messageId, answer: answer, decision: decision,
            fresh: decision == .approved && fresh ? true : nil, feedback: feedback,
            answers: answers, surfaceData: surfaceData
        )
        // EXP-1459: a sealed card is answered by re-sealing it whole.
        if let sealed = await sealer?.sealAnswer(accountId: accountId, input) {
            try await trpc.mutationVoid(accountId: accountId, path: "sessions.answer", input: sealed)
            return
        }
        // #EXP-1641: a sealed card's answer never leaves in clear.
        if sealedOnly { throw SealedOnlyError() }
        try await trpc.mutationVoid(accountId: accountId, path: "sessions.answer", input: input)
    }

    /// #EXP-1561: dismiss a WAITING question or plan card through THE answer
    /// path (waiting-cards.json DISMISS).
    public func dismissCard(accountId: String, messageId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.answer", input: DismissCardInput(messageId: messageId)
        )
    }

    /// Revoke the person's own message the device has not consumed yet.
    public func unqueue(accountId: String, messageId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.unqueue", input: MessageIdInput(messageId: messageId)
        )
    }

    /// EXP-1323: stamp the viewer's read of a session (a chat root on
    /// open, a session view on open) — the Chats list's bold + dot.
    public func markRead(accountId: String, sessionId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.markRead", input: SessionIdInput(sessionId: sessionId)
        )
    }

    /// #EXP-1388: the chat selection's Mark read (`read` true = now) /
    /// Mark unread (the server sets the read just before the newest agent
    /// message). The server takes up to 200 roots per call: bigger
    /// selections go in 200-id batches.
    public func setRead(accountId: String, sessionIds: [String], read: Bool) async throws {
        for batch in Self.selectionBatches(sessionIds) {
            try await trpc.mutationVoid(
                accountId: accountId, path: "sessions.setRead", input: SetReadInput(sessionIds: batch, read: read)
            )
        }
    }

    /// #EXP-1388: archive / unarchive chats for the viewer
    /// (`session_reads.archived_at`), in 200-id batches like `setRead`.
    public func setArchived(accountId: String, sessionIds: [String], archived: Bool) async throws {
        for batch in Self.selectionBatches(sessionIds) {
            try await trpc.mutationVoid(
                accountId: accountId, path: "sessions.setArchived",
                input: SetArchivedInput(sessionIds: batch, archived: archived)
            )
        }
    }

    /// `sessions.setRead` / `setArchived` take up to 200 ids a call.
    static let selectionBatch = 200

    static func selectionBatches(_ ids: [String]) -> [[String]] {
        stride(from: 0, to: ids.count, by: selectionBatch).map { Array(ids[$0..<min($0 + selectionBatch, ids.count)]) }
    }

    /// EXP-1323: start one suggested task — a child (on `deviceId` / with
    /// `model` when picked under Options) or `here` (Do it here).
    public func startSuggestion(
        accountId: String,
        messageId: String,
        itemId: String,
        where target: SuggestionStartWhere,
        deviceId: String? = nil,
        account: String? = nil,
        model: String? = nil
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "sessions.startSuggestion",
            input: StartSuggestionInput(
                messageId: messageId, itemId: itemId, where: target,
                deviceId: deviceId, account: account, model: model
            )
        )
    }

    /// EXP-1323: Start all — every open item of the card as a child.
    public func startAllSuggestions(accountId: String, messageId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.startAllSuggestions", input: MessageIdInput(messageId: messageId)
        )
    }

    public func dismissSuggestion(accountId: String, messageId: String, itemId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "sessions.dismissSuggestion",
            input: SuggestionItemInput(messageId: messageId, itemId: itemId)
        )
    }

    /// EXP-1211: send one quick reply — its text becomes the session's next
    /// message (at once, even mid-turn) and the chip drops.
    public func sendReply(accountId: String, messageId: String, itemId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "sessions.sendReply",
            input: SuggestionItemInput(messageId: messageId, itemId: itemId)
        )
    }

    /// EXP-1211: dismiss the session's quick replies (the whole card).
    public func dismissReplies(accountId: String, messageId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.dismissReplies", input: MessageIdInput(messageId: messageId)
        )
    }

    /// EXP-1323: File as issue — the item becomes an issue linked to the
    /// chat.
    public func fileSuggestion(accountId: String, messageId: String, itemId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "sessions.fileSuggestion",
            input: SuggestionItemInput(messageId: messageId, itemId: itemId)
        )
    }
}

// MARK: - pullRequests

/// `pullRequests.merge`'s answer. `merged == false` carries the reason in
/// `error`; `queued` = GitHub's merge queue took it.
public struct PullRequestMergeResult: Decodable, Sendable, Equatable {
    public let merged: Bool
    public let queued: Bool?
    public let error: String?

    public init(merged: Bool, queued: Bool? = nil, error: String? = nil) {
        self.merged = merged
        self.queued = queued
        self.error = error
    }
}

/// One changed file of a PR (`pullRequests.files`, the Guide's Changes rows).
public struct PullRequestFile: Decodable, Sendable, Equatable, Identifiable {
    public let path: String
    public let additions: Int
    public let deletions: Int
    public let patch: String?

    public var id: String { path }

    public init(path: String, additions: Int, deletions: Int, patch: String? = nil) {
        self.path = path
        self.additions = additions
        self.deletions = deletions
        self.patch = patch
    }
}

private struct PullRequestFilesResult: Decodable {
    let files: [PullRequestFile]
}

/// Internal (not private) so the wire test can pin the omitted `stack`.
struct MergePullRequestInput: Encodable {
    let pullRequestId: String
    /// Merge THROUGH this member of a native stack; nil = the single merge.
    let stack: Bool?
}

private struct PullRequestIdInput: Encodable {
    let pullRequestId: String
}

/// #EXP-1709: `pullRequests.hold` (hold = false: Resume auto-merge / Retry).
private struct HoldPullRequestInput: Encodable {
    let pullRequestId: String
    let hold: Bool
}

public final class PullRequestsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Squash-merge via the GitHub App; `stack` merges the open stack below
    /// it bottom-up, then this one. The state flips land over the shapes.
    @discardableResult
    public func merge(
        accountId: String, pullRequestId: String, stack: Bool? = nil
    ) async throws -> PullRequestMergeResult {
        try await trpc.mutation(
            accountId: accountId,
            path: "pullRequests.merge",
            input: MergePullRequestInput(pullRequestId: pullRequestId, stack: stack)
        )
    }

    /// Close WITHOUT merging.
    public func close(accountId: String, pullRequestId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "pullRequests.close",
            input: PullRequestIdInput(pullRequestId: pullRequestId)
        )
    }

    /// #EXP-1709 (merge-train.json MENU): an owner's Merge now: the PR (its
    /// whole stack) lands at the queue's front, untested.
    public func mergeNow(accountId: String, pullRequestId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "pullRequests.mergeNow",
            input: PullRequestIdInput(pullRequestId: pullRequestId)
        )
    }

    /// #EXP-1709: a member's Hold auto-merge (`hold`) or Resume auto-merge /
    /// the Needs you Retry (`!hold`).
    public func hold(accountId: String, pullRequestId: String, hold: Bool) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "pullRequests.hold",
            input: HoldPullRequestInput(pullRequestId: pullRequestId, hold: hold)
        )
    }

    /// The PR's changed files.
    public func files(accountId: String, pullRequestId: String) async throws -> [PullRequestFile] {
        let result: PullRequestFilesResult = try await trpc.query(
            accountId: accountId,
            path: "pullRequests.files",
            input: PullRequestIdInput(pullRequestId: pullRequestId)
        )
        return result.files
    }
}
