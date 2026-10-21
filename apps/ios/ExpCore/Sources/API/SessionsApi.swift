import Foundation

// Mirrors apps/web/src/lib/trpc/sessions.ts (EXP-1291 Threads v2). A thread
// IS a root session, children are sub-sessions, and (NEW-2) a team agent is
// a definition (`AgentsApi`) whose conversations are roots with `agent_id` —
// and ONE entrypoint starts and messages them all: `sessions.send`. The database is the bus: a new session is a `queued` row
// naming its device, a message a `messages` row; both land here over the
// shapes. Every write answers with a `txid` the client never needs, so it is
// not decoded. The `pullRequests` router (merge/close/files) lives at the
// bottom: PR state left `issues` for the synced `pull_requests` rows.

// MARK: - sessions.send

/// The composer's runner pickers for a NEW session. Nil fields are omitted
/// from the wire and mean the device's own default; `model: ""` / `effort:
/// ""` is an explicit "CLI default".
public struct SessionLaunchOptions: Sendable, Equatable {
    /// Contract `codingAgent` (claude/codex).
    public var cli: String?
    /// The agent login profile on the device (`system` = the ambient login);
    /// nil = its last used one.
    public var account: String?
    public var model: String?
    public var effort: String?
    public var planMode: Bool?
    /// nil = the device's own `launch_defaults.computerUse`.
    public var computerUse: Bool?
    /// Claude only.
    public var ultracode: Bool?
    /// The team MCP servers the session gets; nil = none picked.
    public var mcpServerIds: [String]?

    public init(
        cli: String? = nil,
        account: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        planMode: Bool? = nil,
        computerUse: Bool? = nil,
        ultracode: Bool? = nil,
        mcpServerIds: [String]? = nil
    ) {
        self.cli = cli
        self.account = account
        self.model = model
        self.effort = effort
        self.planMode = planMode
        self.computerUse = computerUse
        self.ultracode = ultracode
        self.mcpServerIds = mcpServerIds
    }
}

/// The four forms of `sessions.send`. `teamId` is the ACTIVE team (phones:
/// the current board's team).
public enum SessionsSendInput: Sendable, Equatable {
    /// A new THREAD root: the composer, Start on an issue (`issueIds`), a
    /// saved prompt run (NEW-2 `promptId`: its body plus `text` = the first
    /// message). Needs `text`, `issueIds` or a `promptId`.
    case newThread(
        teamId: String,
        deviceId: String,
        options: SessionLaunchOptions = SessionLaunchOptions(),
        text: String? = nil,
        attachmentIds: [String]? = nil,
        issueIds: [String]? = nil,
        promptId: String? = nil,
        repositoryId: String? = nil
    )
    /// NEW-2: a message to a team AGENT — into the caller's own conversation
    /// session with it (created on first use; it runs on the agent's own
    /// device, so no runner picks). Needs text or files.
    case agent(agentId: String, text: String? = nil, attachmentIds: [String]? = nil)
    /// A CHILD session of `parentId` on any device.
    case child(
        parentId: String,
        deviceId: String,
        options: SessionLaunchOptions = SessionLaunchOptions(),
        text: String? = nil
    )
    /// A message into an existing session (resumes it when idle; an ended
    /// thread continues in a new child server-side). Needs text or files.
    case message(sessionId: String, text: String? = nil, attachmentIds: [String]? = nil)
    /// EXP-1323: a `/btw` aside — a child of `parentId` asked `text` (the
    /// question) on the parent's own device, outside its conversation.
    case aside(parentId: String, text: String)
    /// EXP-849: the mid-session account SWITCH — `{sessionId, account}`, no
    /// text. The server moves the synced `sessions.account`; the host device
    /// resumes the SAME row on that login between turns.
    case switchAccount(sessionId: String, account: String)
    /// EXP-1252: MOVE the session to another device — `{sessionId, deviceId}`,
    /// no text. The server sets the synced `sessions.move_to_device_id`; the
    /// source host hands the SAME row over after its current turn. The
    /// session's CURRENT device while a move is pending = cancel.
    case moveDevice(sessionId: String, deviceId: String)
}

// Synthesized-style encodeIfPresent per field: every nil is OMITTED, never
// `null` (the router's fields are `.optional()`, not `.nullable()`).
extension SessionsSendInput: Encodable {
    private enum Keys: String, CodingKey {
        case teamId, sessionId, parentId, deviceId
        case cli, account, model, effort, planMode, computerUse, ultracode, mcpServerIds
        case text, attachmentIds, issueIds, promptId, repositoryId, agentId, aside
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        func encodeOptions(_ options: SessionLaunchOptions) throws {
            try c.encodeIfPresent(options.cli, forKey: .cli)
            try c.encodeIfPresent(options.account, forKey: .account)
            try c.encodeIfPresent(options.model, forKey: .model)
            try c.encodeIfPresent(options.effort, forKey: .effort)
            try c.encodeIfPresent(options.planMode, forKey: .planMode)
            try c.encodeIfPresent(options.computerUse, forKey: .computerUse)
            try c.encodeIfPresent(options.ultracode, forKey: .ultracode)
            try c.encodeIfPresent(options.mcpServerIds, forKey: .mcpServerIds)
        }
        switch self {
        case let .newThread(teamId, deviceId, options, text, attachmentIds, issueIds, promptId, repositoryId):
            try c.encode(teamId, forKey: .teamId)
            try c.encode(deviceId, forKey: .deviceId)
            try encodeOptions(options)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(attachmentIds, forKey: .attachmentIds)
            try c.encodeIfPresent(issueIds, forKey: .issueIds)
            try c.encodeIfPresent(promptId, forKey: .promptId)
            try c.encodeIfPresent(repositoryId, forKey: .repositoryId)
        case let .agent(agentId, text, attachmentIds):
            try c.encode(agentId, forKey: .agentId)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(attachmentIds, forKey: .attachmentIds)
        case let .child(parentId, deviceId, options, text):
            try c.encode(parentId, forKey: .parentId)
            try c.encode(deviceId, forKey: .deviceId)
            try encodeOptions(options)
            try c.encodeIfPresent(text, forKey: .text)
        case let .message(sessionId, text, attachmentIds):
            try c.encode(sessionId, forKey: .sessionId)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(attachmentIds, forKey: .attachmentIds)
        case let .switchAccount(sessionId, account):
            try c.encode(sessionId, forKey: .sessionId)
            try c.encode(account, forKey: .account)
        case let .moveDevice(sessionId, deviceId):
            try c.encode(sessionId, forKey: .sessionId)
            try c.encode(deviceId, forKey: .deviceId)
        case let .aside(parentId, text):
            try c.encode(parentId, forKey: .parentId)
            try c.encode(text, forKey: .text)
            try c.encode(true, forKey: .aside)
        }
    }
}

/// `sessions.send`'s answer: the session the call started or messaged, and
/// the `messages` row its text became (nil for a start without text).
public struct SessionsSendResult: Decodable, Sendable, Equatable {
    public let sessionId: String
    public let messageId: String?

    public init(sessionId: String, messageId: String? = nil) {
        self.sessionId = sessionId
        self.messageId = messageId
    }
}

// MARK: - Other inputs

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
    public var planMode: Bool?
    public var effort: String?
    public var ultracode: Bool?
    public var computerUse: Bool?
    public var mcpServerIds: [String]?

    public init(
        sessionId: String,
        planMode: Bool? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        computerUse: Bool? = nil,
        mcpServerIds: [String]? = nil
    ) {
        self.sessionId = sessionId
        self.planMode = planMode
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

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// THE entrypoint: start a thread or a child, or message a session or an
    /// agent. A refusal throws `TrpcError.httpError`, whose description is
    /// the server's message.
    @discardableResult
    public func send(accountId: String, _ input: SessionsSendInput) async throws -> SessionsSendResult {
        try await trpc.mutation(accountId: accountId, path: "sessions.send", input: input)
    }

    /// Interrupt the session's current turn (it stays live, goes idle).
    public func stop(accountId: String, sessionId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.stop", input: SessionIdInput(sessionId: sessionId)
        )
    }

    /// End a session explicitly; ending a ROOT ends its whole tree. Agents
    /// refuse.
    public func end(accountId: String, sessionId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.end", input: SessionIdInput(sessionId: sessionId)
        )
    }

    /// Link issues to a thread (always lands on the ROOT; the header "+").
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
        surfaceData: CardJSON? = nil
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "sessions.answer",
            input: AnswerMessageInput(
                messageId: messageId, answer: answer, decision: decision,
                fresh: decision == .approved && fresh ? true : nil, feedback: feedback,
                answers: answers, surfaceData: surfaceData
            )
        )
    }

    /// Revoke the person's own message the device has not consumed yet.
    public func unqueue(accountId: String, messageId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.unqueue", input: MessageIdInput(messageId: messageId)
        )
    }

    /// EXP-1323: stamp the viewer's read of a session (a thread root on
    /// open, a session view on open) — the Threads list's bold + dot.
    public func markRead(accountId: String, sessionId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "sessions.markRead", input: SessionIdInput(sessionId: sessionId)
        )
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
    /// thread.
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
