import Foundation

/// #FEED-191 NEEDS YOU ×4 (`packages/domain-contract/fixtures/needs-you.json`,
/// TS reference `src/needs-you.ts`, desktop `domain::needs_you`, Android
/// `domain/NeedsYou.kt`): every plan and question waiting on the viewer, plus
/// (#EXP-1699) the PRs ready to merge.
/// `NeedsYouTests` replays every fixture case.
public enum NeedsYou {
    // MARK: - Fixture constants

    public static let route = "/t/$teamSlug/needs-you"

    public enum Copy {
        public static let entry = "Needs you"
        public static let empty = "Nothing needs you"
    }

    public enum Prefix {
        public static let question = "Question: "
    }

    public enum TestIds {
        public static let entry = "nav-needs-you"
        public static let row = "needs-you-row"
        public static let list = "needs-you"
    }

    /// The neutral copy of a mirror's origin the viewer may not read.
    public enum Private {
        public static let plan = "Plan in a private chat"
        public static let question = "Question in a private chat"
    }

    // MARK: - Input

    public struct SessionInput: Equatable, Sendable {
        public let id: String
        public let rootId: String
        /// The session's display name (session-title; an agent conversation = the agent's).
        public let name: String
        public let status: String
        public let agentId: String?
        public let userId: String?
        /// #EXP-1699: the session works now (a PR row waits on a person only when none does).
        public let busy: Bool

        public init(
            id: String, rootId: String, name: String, status: String, agentId: String?, userId: String?,
            busy: Bool = false
        ) {
            self.id = id
            self.rootId = rootId
            self.name = name
            self.status = status
            self.agentId = agentId
            self.userId = userId
            self.busy = busy
        }
    }

    public struct MessageInput: Equatable, Sendable {
        public let id: String
        public let sessionId: String
        public let authorSessionId: String?
        public let body: String
        /// Wire timestamps, as the digests carry them.
        public let createdAt: String
        public let answeredAt: String?
        public let card: TreeMessageCard?

        public init(
            id: String, sessionId: String, authorSessionId: String?, body: String, createdAt: String,
            answeredAt: String?, card: TreeMessageCard?
        ) {
            self.id = id
            self.sessionId = sessionId
            self.authorSessionId = authorSessionId
            self.body = body
            self.createdAt = createdAt
            self.answeredAt = answeredAt
            self.card = card
        }
    }

    /// #EXP-1699: the pull requests shape's row, as far as the fold reads it.
    public struct PullRequestInput: Equatable, Sendable {
        public let id: String
        public let rootSessionId: String?
        public let number: Int
        public let title: String
        public let state: String
        public let checksState: String?
        public let basePullRequestId: String?
        public let createdAt: String
        /// #EXP-1710 (merge-train.json NEEDS YOU): the train's state, wait and
        /// hold reason; nil = a pre-train row.
        public let mergeState: String?
        public let mergeWait: String?
        public let mergeHold: String?
        /// When the opener session's device went offline (wire), nil = online.
        public let deviceOfflineSince: String?

        public init(
            id: String, rootSessionId: String?, number: Int, title: String, state: String, checksState: String?,
            basePullRequestId: String?, createdAt: String,
            mergeState: String? = nil, mergeWait: String? = nil, mergeHold: String? = nil,
            deviceOfflineSince: String? = nil
        ) {
            self.id = id
            self.rootSessionId = rootSessionId
            self.number = number
            self.title = title
            self.state = state
            self.checksState = checksState
            self.basePullRequestId = basePullRequestId
            self.createdAt = createdAt
            self.mergeState = mergeState
            self.mergeWait = mergeWait
            self.mergeHold = mergeHold
            self.deviceOfflineSince = deviceOfflineSince
        }
    }

    // MARK: - Output

    public enum Kind: String, Equatable, Sendable {
        case plan, question, pr
    }

    /// `plan` = the chat's Plan tab; `card` = its card in the Conversation;
    /// `results` = its Results tab; `chat` = an agent conversation.
    public enum Opens: String, Equatable, Sendable {
        case plan, card, results, chat
    }

    public struct Row: Equatable, Sendable, Identifiable {
        /// The waiting card's message (the origin's for a mirror it may read), or the PR's id.
        public let messageId: String
        public let sessionId: String
        public let rootId: String
        public let kind: Kind
        public let opens: Opens
        public let title: String
        public let chat: String
        public let meta: String
        public let age: String
        public let createdAt: String
        /// #EXP-1710: a stuck PR's Button sm row (Retry, Merge now, Close
        /// PR); empty = a plain row.
        public let actions: [MergeTrain.StuckRow.Action]
        /// #EXP-1710: the PR's number (a stuck row's confirms name it).
        public let number: Int?
        public var id: String { messageId }

        public init(
            messageId: String, sessionId: String, rootId: String, kind: Kind, opens: Opens, title: String,
            chat: String, meta: String, age: String, createdAt: String,
            actions: [MergeTrain.StuckRow.Action] = [], number: Int? = nil
        ) {
            self.actions = actions
            self.number = number
            self.messageId = messageId
            self.sessionId = sessionId
            self.rootId = rootId
            self.kind = kind
            self.opens = opens
            self.title = title
            self.chat = chat
            self.meta = meta
            self.age = age
            self.createdAt = createdAt
        }
    }

    // MARK: - The fold

    private static func ms(_ wire: String) -> Double { SessionTree.ms(wire) ?? 0 }

    private static func rowOf(
        _ message: MessageInput, sessions: [String: SessionInput],
        /// Every session the viewer may read, ended ones included.
        readable: [String: SessionInput], now: Double
    ) -> Row? {
        guard let card = message.card, let session = sessions[message.sessionId],
              let root = sessions[session.rootId]
        else { return nil }
        let age = ResultsTab.age(ms(message.createdAt), now: now)
        if card.kind == "plan" || card.kind == "question" {
            // A digest's card is cut (no `decision`): `answeredAt` settles it.
            guard message.answeredAt == nil, WaitingCards.cardState(card, answeredAt: message.answeredAt) == .waiting else { return nil }
            let plan = card.kind == "plan"
            let question = AskDock.title(card.question ?? "")
            return Row(
                messageId: message.id,
                sessionId: session.id,
                rootId: root.id,
                kind: plan ? .plan : .question,
                opens: plan ? .plan : .card,
                title: plan ? PlanPage.planRowLabel(card.plan ?? "") : question.isEmpty ? AskDock.label(.question) : question,
                chat: root.name,
                meta: "",
                age: age,
                createdAt: message.createdAt
            )
        }
        guard card.kind == "ask", message.answeredAt == nil else { return nil }
        // A mirror: the origin's own ids where the viewer may read it, else the
        // mirror's, with the neutral copy (chats.json `askPreview`). A digest from
        // before it kept `messageId` opens the origin's chat, no card to reach.
        let originSession = (card.sessionId ?? message.authorSessionId).flatMap { readable[$0] }
        let preview = Chats.askPreview(
            TreeMessage(
                id: message.id, sessionId: message.sessionId, authorKind: "agent", authorId: nil, body: message.body,
                card: card, answeredAt: message.answeredAt, consumedAt: nil, createdAt: message.createdAt,
                authorSessionId: message.authorSessionId
            ),
            visible: Set(readable.keys)
        )
        let opaque = preview.text == Private.plan || preview.text == Private.question
        let target = opaque ? nil : originSession
        let named = target == nil ? nil : card.messageId
        let plan = preview.kind == .plan
        let title = preview.text.hasPrefix(Prefix.question)
            ? String(preview.text.dropFirst(Prefix.question.count))
            : preview.text
        return Row(
            messageId: named ?? message.id,
            sessionId: target?.id ?? session.id,
            rootId: target?.rootId ?? root.id,
            kind: plan ? .plan : .question,
            opens: plan && named != nil ? .plan : .card,
            title: title,
            chat: target.flatMap { readable[$0.rootId]?.name } ?? root.name,
            meta: "",
            age: age,
            createdAt: message.createdAt
        )
    }

    /// #EXP-1699: the PRs ready to merge (`prCases`), and (#EXP-1710,
    /// merge-train.json NEEDS YOU) the STUCK PRs: `auto` mode lists no ready
    /// PR (nobody waits on a person), `approval` mode (or no mode: a
    /// pre-train caller) keeps them, their tap being Needs approval.
    private static func pullRequestRows(
        now: Double, me: String?, sessions: [SessionInput], pullRequests: [PullRequestInput],
        mergeMode: String?, owner: Bool
    ) -> [Row] {
        let sessionsById = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let live = Set(pullRequests.filter { $0.state == "open" || $0.state == "draft" }.map(\.id))
        let working = Set(sessions.filter(\.busy).map(\.rootId))
        let nowDate = Date(timeIntervalSince1970: now / 1000)
        return pullRequests.compactMap { pr in
            guard let root = pr.rootSessionId.flatMap({ sessionsById[$0] }) else { return nil }
            if root.agentId != nil, root.userId != me { return nil }
            if pr.state == "open" || pr.state == "draft",
               let stuck = MergeTrain.stuckRow(
                   number: pr.number, state: pr.mergeState, holdReason: pr.mergeHold,
                   deviceOfflineSince: pr.deviceOfflineSince.flatMap(WireTimestamps.parse), now: nowDate, owner: owner
               ) {
                return Row(
                    messageId: pr.id,
                    sessionId: root.id,
                    rootId: root.id,
                    kind: .pr,
                    opens: root.agentId == nil ? .results : .chat,
                    title: stuck.title,
                    chat: root.name,
                    meta: stuck.meta,
                    age: ResultsTab.age(ms(pr.createdAt), now: now),
                    createdAt: pr.createdAt,
                    actions: stuck.actions,
                    number: pr.number
                )
            }
            guard mergeMode != DomainContract.teamMergeModeAuto, pr.state == "open", pr.checksState == "success"
            else { return nil }
            // A train row already past the approval wait never asks again.
            if let state = pr.mergeState, state != DomainContract.prMergeStateChecks { return nil }
            if let wait = pr.mergeWait, wait != "approval" { return nil }
            if working.contains(root.id) { return nil }
            if let base = pr.basePullRequestId, live.contains(base) { return nil }
            return Row(
                messageId: pr.id,
                sessionId: root.id,
                rootId: root.id,
                kind: .pr,
                opens: root.agentId == nil ? .results : .chat,
                title: pr.title,
                chat: root.name,
                meta: "#\(pr.number)",
                age: ResultsTab.age(ms(pr.createdAt), now: now),
                createdAt: pr.createdAt,
                number: pr.number
            )
        }
    }

    /// THE fold: one row per waiting card, once, plus (#EXP-1699) the PRs ready
    /// to merge, newest first.
    /// `mergeMode` = the team's (contract teamMergeMode; nil = a pre-train
    /// caller: the EXP-1699 rows stay); `owner` = the viewer owns the team
    /// (a stuck row's Merge now).
    public static func rows(
        now: Double, me: String?, sessions: [SessionInput], messages: [MessageInput],
        pullRequests: [PullRequestInput] = [], mergeMode: String? = nil, owner: Bool = false
    ) -> [Row] {
        // Another member's agent conversation is theirs (chats.json `listAgents`).
        let mayRead = sessions.filter { $0.agentId == nil || $0.userId == me }
        let readable = Dictionary(mayRead.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let live = readable.filter { $0.value.status != "ended" }
        var rows: [String: Row] = [:]
        var seen = Set<String>()
        var unnamed: [Row] = []
        for message in messages where seen.insert(message.id).inserted {
            guard let row = rowOf(message, sessions: live, readable: readable, now: now) else { continue }
            if message.id == row.messageId, row.sessionId != message.sessionId {
                // An older digest's mirror naming only a session waits until every card is known.
                unnamed.append(row)
            } else if message.id == row.messageId || rows[row.messageId] == nil {
                // The card itself beats its mirror's copy of it.
                rows[row.messageId] = row
            }
        }
        var waitingIn = Set(rows.values.map(\.sessionId))
        for row in unnamed where waitingIn.insert(row.sessionId).inserted {
            rows[row.messageId] = row
        }
        let all = Array(rows.values) + pullRequestRows(
            now: now, me: me, sessions: sessions, pullRequests: pullRequests, mergeMode: mergeMode, owner: owner
        )
        return all.sorted { a, b in
            if (a.kind == .pr) != (b.kind == .pr) { return b.kind == .pr }
            let at = ms(a.createdAt), bt = ms(b.createdAt)
            return at != bt ? at > bt : a.messageId > b.messageId
        }
    }
}
