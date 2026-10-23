import Foundation

// EXP-1291 Threads v2: the session tree's view models — a thread IS a root
// session, an agent's conversation a root with `agentId` (NEW-2), children
// are sub-sessions. A 1:1 port of the TS reference
// `packages/domain-contract/src/session-tree.ts` (desktop
// `domain::session_tree`, Android `domain/SessionTree.kt`), byte-locked by the
// fixtures `SessionTreeTests` replays:
//   fixtures/session-card.json   → `SessionTree.sessionCard` (names = `SessionTitle`)
//   fixtures/session-thread.json → `SessionTree.buildSessionThread`
//   fixtures/threads-list.json   → `SessionTree.partitionThreads` + `listAgents` + `threadRowMarkState`
// The fixtures' `$comment`s hold the rules. Pure: callers map their synced
// rows into the `Tree*` inputs.

// MARK: - Inputs

/// A usage wall (`sessions.blocked` jsonb): set = the session is walled.
/// NEW-1: `kind` = why (contract codingSessionBlocked.kinds: `rate_limit`,
/// or a watchdog flag `device_offline` / `unclaimed` / `stuck`); nil = a
/// usage wall written before the column carried it.
public struct TreeSessionBlocked: Equatable, Sendable {
    public let kind: String?
    public let window: String?
    public let resetsAt: String?

    public init(kind: String? = nil, window: String? = nil, resetsAt: String? = nil) {
        self.kind = kind
        self.window = window
        self.resetsAt = resetsAt
    }

    /// The synced jsonb TEXT; nil, `null` or a non-object = not walled.
    public static func parse(_ raw: String?) -> TreeSessionBlocked? {
        guard let object = SessionTree.jsonObject(raw) else { return nil }
        return TreeSessionBlocked(
            kind: object["kind"] as? String,
            window: object["window"] as? String,
            resetsAt: object["resetsAt"] as? String
        )
    }
}

/// One `sessions.guide` entry, as far as the tree reads it (the Guide tab's
/// dot). The Guide tab itself renders through `SessionResults`.
public struct TreeGuideEntry: Equatable, Sendable {
    public let text: String?
    public let label: String?

    public init(text: String? = nil, label: String? = nil) {
        self.text = text
        self.label = label
    }

    /// The synced jsonb TEXT; nil/`null`/a non-array = no guide.
    public static func parseList(_ raw: String?) -> [TreeGuideEntry]? {
        guard let raw, let data = raw.data(using: .utf8),
              let entries = (try? JSONSerialization.jsonObject(with: data)) as? [Any]
        else { return nil }
        return entries.compactMap { entry in
            guard let object = entry as? [String: Any] else { return nil }
            return TreeGuideEntry(
                text: object["text"] as? String,
                label: object["label"] as? String
            )
        }
    }
}

/// One `sessions` row (root or child).
public struct TreeSession: Equatable, Sendable, Identifiable {
    public let id: String
    public let teamId: String
    /// nil = a root.
    public let parentId: String?
    /// The root's id (a root names itself).
    public let rootId: String
    public let userId: String
    /// Contract `sessionStatusValues`: queued | running | idle | ended.
    public let status: String
    public let busy: Bool
    public let needsInput: Bool
    public let blocked: TreeSessionBlocked?
    public let shared: Bool
    public let title: String?
    public let guide: [TreeGuideEntry]?
    public let startedAt: String?
    public let endedAt: String?
    public let createdAt: String
    public let updatedAt: String
    /// EXP-1316: the steer device id of the machine that runs it (the
    /// thread row's device glyph); nil when unknown.
    public let deviceId: String?
    /// NEW-2: the agent this root is a conversation of (nil = a thread).
    public let agentId: String?
    /// VAPP-110: the app this root is a member's editor session of (nil =
    /// none); like an agent's conversation it is no thread.
    public let appId: String?

    public init(
        id: String,
        teamId: String,
        parentId: String?,
        rootId: String,
        userId: String,
        status: String,
        busy: Bool,
        needsInput: Bool,
        blocked: TreeSessionBlocked?,
        shared: Bool,
        title: String?,
        guide: [TreeGuideEntry]?,
        startedAt: String?,
        endedAt: String?,
        createdAt: String,
        updatedAt: String,
        deviceId: String? = nil,
        agentId: String? = nil,
        appId: String? = nil
    ) {
        self.agentId = agentId
        self.appId = appId
        self.deviceId = deviceId
        self.id = id
        self.teamId = teamId
        self.parentId = parentId
        self.rootId = rootId
        self.userId = userId
        self.status = status
        self.busy = busy
        self.needsInput = needsInput
        self.blocked = blocked
        self.shared = shared
        self.title = title
        self.guide = guide
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

/// EXP-1323: one of the viewer's read marks (`session_reads`, user-scoped).
public struct TreeSessionRead: Equatable, Sendable {
    public let sessionId: String
    public let readAt: String

    public init(sessionId: String, readAt: String) {
        self.sessionId = sessionId
        self.readAt = readAt
    }
}

/// One `session_issues` link (session ↔ issue). NEW-1: the row sits on the
/// session that HOLDS the issue (root or child alike).
public struct TreeSessionIssue: Equatable, Sendable {
    public let sessionId: String
    public let issueId: String
    public let sortOrder: Double

    public init(sessionId: String, issueId: String, sortOrder: Double) {
        self.sessionId = sessionId
        self.issueId = issueId
        self.sortOrder = sortOrder
    }
}

public struct TreeIssue: Equatable, Sendable, Identifiable {
    public let id: String
    public let identifier: String
    public let title: String
    /// EXP-1334: the issue's parent (`issue_relations` kind parent): a linked
    /// sub-issue folds under its nearest LINKED ancestor.
    public let parentId: String?

    public init(id: String, identifier: String, title: String, parentId: String? = nil) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.parentId = parentId
    }
}

/// One question-card option (EXP-1322): the answer IS the label; the
/// description (nil when absent) renders under it. The synced card carries a
/// bare string or `{label, description?}`; both normalise here.
public struct TreeQuestionOption: Equatable, Sendable {
    public let label: String
    public let description: String?
    /// EXP-1280: the option's picture (plan-card.json `pictureFor`); nil = a
    /// text option.
    public let attachmentId: String?

    public init(label: String, description: String? = nil, attachmentId: String? = nil) {
        self.label = label
        self.description = description
        self.attachmentId = attachmentId
    }
}

extension TreeQuestionOption: ExpressibleByStringLiteral {
    public init(stringLiteral value: String) {
        self.init(label: value)
    }
}

/// EXP-1323: one raw `suggestions` card item (domain `SuggestionItem`).
public struct TreeSuggestionItem: Equatable, Sendable {
    public let id: String
    public let title: String
    public let text: String
    public let sessionId: String?
    public let issueId: String?
    public let here: Bool
    public let dismissed: Bool
    /// EXP-1211 `replies` item: already sent as a message.
    public let sent: Bool
    /// EXP-1211: `text` was a JSON string (a coerced scalar never renders as a reply).
    public let textIsString: Bool

    public init(
        id: String,
        title: String,
        text: String,
        sessionId: String? = nil,
        issueId: String? = nil,
        here: Bool = false,
        dismissed: Bool = false,
        sent: Bool = false,
        textIsString: Bool = true
    ) {
        self.sent = sent
        self.textIsString = textIsString
        self.id = id
        self.title = title
        self.text = text
        self.sessionId = sessionId
        self.issueId = issueId
        self.here = here
        self.dismissed = dismissed
    }
}

/// A message's `card` jsonb (contract `messageCardKindValues`), read
/// tolerantly: every field the rows use, nil when absent or mistyped.
public struct TreeMessageCard: Equatable, Sendable {
    public let kind: String?
    /// session | thread_started.
    public let sessionId: String?
    /// question.
    public let question: String?
    public let options: [TreeQuestionOption]
    public let answer: String?
    /// EXP-1280 question: several picks; free text offered (default true);
    /// the picked labels (nil on a legacy card = `answer` alone).
    public let multi: Bool
    public let custom: Bool
    public let labels: [String]?
    /// plan.
    public let plan: String?
    public let decision: String?
    /// An approval that started in a fresh context (ask-dock.json).
    public let fresh: Bool
    /// EXP-1280 plan: the choice questions, the picked labels per question,
    /// the note.
    public let questions: [ThreadPlanQuestion]
    public let planAnswers: [String: [String]]?
    public let feedback: String?
    /// EXP-1280 plan + question: the inline Exponential UI surface, and its
    /// data model as the person answered it.
    public let surface: CardSurface?
    public let surfaceData: CardJSON?
    /// The open ACP ask a device mirrored (nil = a tool-posted card,
    /// `exponential_plan_propose` / `exponential_sessions_ask`, EXP-1280).
    public let questionId: String?
    /// subagent.
    public let title: String?
    public let status: String?
    /// picture.
    public let attachmentId: String?
    public let width: Double?
    public let height: Double?
    public let caption: String?
    /// event (contract `messageEventKindValues`).
    public let event: String?
    public let issueId: String?
    public let pullRequestId: String?
    /// EXP-1323 report.
    public let summary: String?
    /// EXP-1323 suggestions: nil = no items array (no row).
    public let items: [TreeSuggestionItem]?
    /// NEW-2 `trigger_fired`: the event name the card's `name` carries; nil =
    /// none (a schedule firing draws no line).
    public let eventName: String?

    public init(
        kind: String?,
        sessionId: String? = nil,
        question: String? = nil,
        options: [TreeQuestionOption] = [],
        answer: String? = nil,
        multi: Bool = false,
        custom: Bool = true,
        labels: [String]? = nil,
        plan: String? = nil,
        decision: String? = nil,
        fresh: Bool = false,
        questions: [ThreadPlanQuestion] = [],
        planAnswers: [String: [String]]? = nil,
        feedback: String? = nil,
        surface: CardSurface? = nil,
        surfaceData: CardJSON? = nil,
        questionId: String? = nil,
        title: String? = nil,
        status: String? = nil,
        attachmentId: String? = nil,
        width: Double? = nil,
        height: Double? = nil,
        caption: String? = nil,
        event: String? = nil,
        issueId: String? = nil,
        pullRequestId: String? = nil,
        summary: String? = nil,
        items: [TreeSuggestionItem]? = nil,
        eventName: String? = nil
    ) {
        self.kind = kind
        self.eventName = eventName
        self.summary = summary
        self.items = items
        self.sessionId = sessionId
        self.question = question
        self.options = options
        self.answer = answer
        self.multi = multi
        self.custom = custom
        self.labels = labels
        self.plan = plan
        self.decision = decision
        self.fresh = fresh
        self.questions = questions
        self.planAnswers = planAnswers
        self.feedback = feedback
        self.surface = surface
        self.surfaceData = surfaceData
        self.questionId = questionId
        self.title = title
        self.status = status
        self.attachmentId = attachmentId
        self.width = width
        self.height = height
        self.caption = caption
        self.event = event
        self.issueId = issueId
        self.pullRequestId = pullRequestId
    }

    /// A decoded JSON object (JSONSerialization).
    public init(object: [String: Any]) {
        func text(_ key: String) -> String? { object[key] as? String }
        // `String(card.x ?? '')` in the reference: a scalar reads as text.
        func loose(_ key: String) -> String? {
            switch object[key] {
            case let string as String: string
            case let number as NSNumber: number.stringValue
            default: nil
            }
        }
        func number(_ key: String) -> Double? {
            guard let value = object[key] as? NSNumber,
                  CFGetTypeID(value) != CFBooleanGetTypeID()
            else { return nil }
            return value.doubleValue
        }
        let pictures = PlanCardPicture.list(object["pictures"])
        func sidecar(_ key: String, _ index: Int) -> String? {
            guard let list = object[key] as? [Any], index < list.count,
                  let value = list[index] as? String, !value.isEmpty
            else { return nil }
            return value
        }
        self.init(
            kind: text("kind"),
            sessionId: text("sessionId"),
            question: loose("question"),
            // EXP-1322: `options` = plain labels, descriptions ride the
            // parallel `optionDescriptions` sidecar (null / "" = none), zipped
            // by ORIGINAL index before non-string options drop. EXP-1280:
            // pictures ride `optionPictures` (labels resolved through
            // `pictures`) the same way.
            options: (object["options"] as? [Any])?.enumerated().compactMap { index, option -> TreeQuestionOption? in
                guard let label = option as? String else { return nil }
                return TreeQuestionOption(
                    label: label,
                    description: sidecar("optionDescriptions", index),
                    attachmentId: PlanCard.pictureFor(pictures, label: sidecar("optionPictures", index))
                )
            } ?? [],
            answer: text("answer"),
            multi: CardJSON.isTrue(object["multi"]),
            custom: !CardJSON.isFalse(object["custom"]),
            labels: (object["answers"] as? [Any]).map { $0.compactMap { $0 as? String } },
            plan: loose("plan"),
            decision: text("decision"),
            // `card.fresh === true` in the reference.
            fresh: (object["fresh"] as? NSNumber).map { CFGetTypeID($0) == CFBooleanGetTypeID() && $0.boolValue } ?? false,
            questions: ThreadPlanQuestion.list(object["questions"], pictures: pictures),
            planAnswers: (object["answers"] as? [String: Any]).map { raw in
                var answers: [String: [String]] = [:]
                for (id, labels) in raw {
                    guard let labels = labels as? [Any] else { continue }
                    answers[id] = labels.compactMap { $0 as? String }
                }
                return answers
            },
            feedback: text("feedback").flatMap { $0.isEmpty ? nil : $0 },
            surface: CardSurface.parse(object["surface"]),
            surfaceData: (object["surfaceData"] as? [String: Any]).flatMap(CardJSON.init(any:)),
            questionId: text("questionId"),
            title: loose("title"),
            status: loose("status"),
            attachmentId: text("attachmentId"),
            width: number("width"),
            height: number("height"),
            caption: text("caption"),
            event: loose("event"),
            issueId: text("issueId"),
            pullRequestId: text("pullRequestId"),
            summary: text("summary"),
            // `String(raw.x ?? '')`: a scalar reads as text, absent/null = "".
            items: (object["items"] as? [Any]).map { raw in
                raw.compactMap { entry -> TreeSuggestionItem? in
                    guard let item = entry as? [String: Any], let id = item["id"] as? String else { return nil }
                    func scalar(_ key: String) -> String {
                        switch item[key] {
                        case let string as String: string
                        case let number as NSNumber:
                            CFGetTypeID(number) == CFBooleanGetTypeID()
                                ? (number.boolValue ? "true" : "false")
                                : number.stringValue
                        default: ""
                        }
                    }
                    return TreeSuggestionItem(
                        id: id,
                        title: scalar("title"),
                        text: scalar("text"),
                        sessionId: item["sessionId"] as? String,
                        issueId: item["issueId"] as? String,
                        here: (item["here"] as? Bool) == true,
                        dismissed: (item["dismissed"] as? Bool) == true,
                        sent: (item["sent"] as? NSNumber).map { CFGetTypeID($0) == CFBooleanGetTypeID() && $0.boolValue } ?? false,
                        textIsString: item["text"] is String
                    )
                }
            },
            eventName: text("name")
        )
    }

    /// The synced jsonb TEXT; nil/`null`/a non-object = no card.
    public static func parse(_ raw: String?) -> TreeMessageCard? {
        SessionTree.jsonObject(raw).map(TreeMessageCard.init(object:))
    }
}

/// EXP-1211: one quick reply above the session composer.
public struct ComposerReply: Equatable, Sendable, Identifiable {
    public let id: String
    public let text: String

    public init(id: String, text: String) {
        self.id = id
        self.text = text
    }
}

/// EXP-1211: `composerReplies` — the newest replies card's open items.
public struct ComposerReplies: Equatable, Sendable {
    public let messageId: String
    public let items: [ComposerReply]

    public init(messageId: String, items: [ComposerReply]) {
        self.messageId = messageId
        self.items = items
    }
}

/// One `messages` row.
public struct TreeMessage: Equatable, Sendable, Identifiable {
    public let id: String
    /// NEW-8: the session its conversation belongs to, keyed through the
    /// `conversations` shape (`MessageConversations`), never the row's own
    /// legacy `session_id`.
    public let sessionId: String
    /// Contract `messageAuthorKindValues`: user | agent | system.
    public let authorKind: String
    public let authorId: String?
    public let body: String
    public let card: TreeMessageCard?
    public let answeredAt: String?
    public let consumedAt: String?
    public let createdAt: String
    /// EXP-1323: the session that posted it (nil = none).
    public let authorSessionId: String?

    public init(
        id: String,
        sessionId: String,
        authorKind: String,
        authorId: String?,
        body: String,
        card: TreeMessageCard?,
        answeredAt: String?,
        consumedAt: String?,
        createdAt: String,
        authorSessionId: String? = nil
    ) {
        self.authorSessionId = authorSessionId
        self.id = id
        self.sessionId = sessionId
        self.authorKind = authorKind
        self.authorId = authorId
        self.body = body
        self.card = card
        self.answeredAt = answeredAt
        self.consumedAt = consumedAt
        self.createdAt = createdAt
    }
}

public struct TreePullRequest: Equatable, Sendable, Identifiable {
    public let id: String
    public let number: Int
    /// NEW-1: `open` | `merged` | `closed` (`treeState` reads it; nil = unknown).
    public let state: String?

    public init(id: String, number: Int, state: String? = nil) {
        self.id = id
        self.number = number
        self.state = state
    }
}

// MARK: - Outputs

/// One state word per session: the card's kind word, icon and tone.
public enum SessionState: String, Equatable, Sendable, CaseIterable {
    case queued
    case working
    case waiting
    case needsYou = "needs_you"
    case blocked
    case ended

    /// Fixture `copy.states`.
    public var word: String {
        switch self {
        case .queued: "Queued"
        case .working: "Working"
        case .waiting: "Waiting"
        case .needsYou: "Needs you"
        case .blocked: "Usage limit"
        case .ended: "Ended"
        }
    }

    /// Fixture `icons` (concept names).
    public var icon: String {
        switch self {
        case .queued: "ui-clock"
        case .working: "coding-running"
        case .waiting: "coding-assistant"
        case .needsYou: "coding-needs-input"
        case .blocked: "notification-session-blocked"
        case .ended: "coding-ended"
        }
    }

    /// Fixture `tones`: muted | foreground | amber | rose.
    public var tone: String {
        switch self {
        case .working: "foreground"
        case .needsYou: "amber"
        case .blocked: "rose"
        case .queued, .waiting, .ended: "muted"
        }
    }
}

/// THE session card = the Work card's header row (EXP-1240 `WorkCardView`):
/// icon, name, the muted state word, trailing elapsed, a spinner while
/// `running`. Tapping it opens that session's raw view.
public struct SessionCard: Equatable, Sendable {
    public let sessionId: String
    /// Concept name.
    public let icon: String
    public let name: String
    public let kindWord: String
    public let state: SessionState
    public let tone: String
    public let elapsed: String?
    public let running: Bool
    public let testId: String

    public init(
        sessionId: String,
        icon: String,
        name: String,
        kindWord: String,
        state: SessionState,
        tone: String,
        elapsed: String?,
        running: Bool,
        testId: String
    ) {
        self.sessionId = sessionId
        self.icon = icon
        self.name = name
        self.kindWord = kindWord
        self.state = state
        self.tone = tone
        self.elapsed = elapsed
        self.running = running
        self.testId = testId
    }
}

/// EXP-1334: `SessionTree.partitionThreadStrip`'s answer: `full` =
/// identifier + title; `short` = identifiers alone; `fold` = short and some
/// chips behind a "+N". Indexes = the chips drawn, strip order.
public struct ThreadStripLayout: Equatable, Sendable {
    public enum Tier: String, Sendable { case full, short, fold }
    public let tier: Tier
    public let issues: [Int]
    public let sessions: [Int]

    public init(tier: Tier, issues: [Int], sessions: [Int]) {
        self.tier = tier
        self.issues = issues
        self.sessions = sessions
    }
}

/// EXP-1334: one row of a folded issue chip's subtree; `depth` is relative
/// to the chip (its children = 1).
public struct ThreadTabNestedIssue: Equatable, Sendable {
    public let issueId: String
    public let identifier: String
    public let depth: Int

    public init(issueId: String, identifier: String, depth: Int) {
        self.issueId = issueId
        self.identifier = identifier
        self.depth = depth
    }
}

/// EXP-1334: one row of a folded session chip's subtree (THE tree rows).
public struct ThreadTabNestedSession: Equatable, Sendable {
    public let sessionId: String
    public let name: String
    public let markState: SessionState
    public let depth: Int

    public init(sessionId: String, name: String, markState: SessionState, depth: Int) {
        self.sessionId = sessionId
        self.name = name
        self.markState = markState
        self.depth = depth
    }
}

/// The thread header's tab strip (EXP-1334: + · Conversation · issue chips ·
/// session chips · Guide; nothing scrolls).
public enum ThreadTab: Equatable, Sendable {
    /// The "+" (link/file an issue), LEFT of Conversation.
    case add
    case conversation
    /// A TOP-LEVEL linked issue; sub-issues fold into `nested`.
    case issue(issueId: String, identifier: String, nested: [ThreadTabNestedIssue])
    /// EXP-1334: one chip per DIRECT child of the dispatcher (the root has
    /// no chip); every deeper session folds into `nested`. `markState` =
    /// `threadRowMarkState` of that session, `.ended` once it ended.
    case session(sessionId: String, name: String, markState: SessionState, nested: [ThreadTabNestedSession])
    /// `dot` while the root's guide holds any text or picture.
    case guide(dot: Bool)
}

/// One Conversation row.
public enum ThreadRow: Equatable, Sendable {
    /// `author` = user | agent | system; a person's row is `queued` until
    /// the device consumed it.
    /// `authorSessionId` (EXP-1323) = the session that posted it (a child
    /// asking its parent).
    case message(id: String, author: String, authorId: String?, body: String, queued: Bool, authorSessionId: String?)
    /// `id` = the root's id for its own card, else the card message's id.
    case session(id: String, card: SessionCard)
    /// EXP-1323: a child's end report — `text` = `Report from <name>`.
    case report(id: String, sessionId: String, text: String, summary: String)
    /// EXP-1323: ONE suggested-tasks card; `sessionId` = the posting session
    /// (Do it here messages it).
    case suggestions(id: String, sessionId: String, header: String, startAll: Bool, items: [SuggestionRow])
    /// A question card (EXP-1280: rich, plan-card.json).
    case question(ThreadQuestionRow)
    /// A plan card (EXP-1280: rich, plan-card.json).
    case plan(ThreadPlanRow)
    case subagent(id: String, title: String, status: String)
    case picture(id: String, attachmentId: String, width: Double?, height: Double?, caption: String?)
    /// One muted line.
    case event(id: String, event: String, text: String)

    /// A `question` / `plan` card message as its row (nil = another kind).
    public static func askCard(_ message: TreeMessage) -> ThreadRow? {
        guard let card = message.card else { return nil }
        switch card.kind {
        case "question":
            return .question(ThreadQuestionRow(
                id: message.id,
                sessionId: message.sessionId,
                question: card.question ?? "",
                options: card.options,
                custom: card.custom,
                multi: card.multi,
                surface: card.surface,
                answer: card.answer,
                answers: card.labels,
                surfaceData: card.surfaceData,
                answered: message.answeredAt != nil
            ))
        case "plan":
            return .plan(ThreadPlanRow(
                id: message.id,
                sessionId: message.sessionId,
                plan: card.plan ?? "",
                questions: card.questions,
                surface: card.surface,
                decision: card.decision,
                fresh: card.fresh,
                answers: card.planAnswers,
                feedback: card.feedback,
                surfaceData: card.surfaceData
            ))
        default:
            return nil
        }
    }

    public var id: String {
        switch self {
        case let .question(row): row.id
        case let .plan(row): row.id
        case let .message(id, _, _, _, _, _),
             let .session(id, _),
             let .report(id, _, _, _),
             let .suggestions(id, _, _, _, _),
             let .subagent(id, _, _),
             let .picture(id, _, _, _, _),
             let .event(id, _, _):
            id
        }
    }
}

/// A question card row. `custom` = a free-text answer is offered beside the
/// options (unless the card says `custom: false`, EXP-1280). `sessionId`
/// (EXP-1334) = the session whose card it is (the root, or a child whose own
/// ask surfaces in the Conversation).
public struct ThreadQuestionRow: Equatable, Sendable, Identifiable {
    public let id: String
    public let sessionId: String
    public let question: String
    public let options: [TreeQuestionOption]
    public let custom: Bool
    /// EXP-1280: several picks allowed.
    public let multi: Bool
    /// EXP-1280: the inline Exponential UI surface, if any.
    public let surface: CardSurface?
    public let answer: String?
    /// EXP-1280: the picked labels (nil on a legacy card = `answer` alone).
    public let answers: [String]?
    /// EXP-1280: the surface's data model as the person answered it.
    public let surfaceData: CardJSON?
    public let answered: Bool

    public init(
        id: String,
        sessionId: String,
        question: String,
        options: [TreeQuestionOption],
        custom: Bool = true,
        multi: Bool = false,
        surface: CardSurface? = nil,
        answer: String? = nil,
        answers: [String]? = nil,
        surfaceData: CardJSON? = nil,
        answered: Bool
    ) {
        self.id = id
        self.sessionId = sessionId
        self.question = question
        self.options = options
        self.custom = custom
        self.multi = multi
        self.surface = surface
        self.answer = answer
        self.answers = answers
        self.surfaceData = surfaceData
        self.answered = answered
    }

    /// The labels shown picked (web `closedPicks`): `answers`, else a legacy
    /// `answer` that names an option. A bare answer that is no option (free
    /// text, or an old joined multi answer "A, B") highlights nothing and
    /// reads as the answer line (`closedAnswerLine`).
    public var picked: [String] {
        PlanCard.closedPicks(options: options, answer: answer, answers: answers)
    }

    /// A closed card's resolution line: the picks that are no option (a typed
    /// answer), else — with nothing picked — the legacy `answer` itself.
    public var closedAnswerLine: [String] {
        let picks = picked
        if !picks.isEmpty { return picks.filter { pick in !options.contains { $0.label == pick } } }
        if let answer, !answer.isEmpty { return [answer] }
        return []
    }
}

/// A plan card row. `fresh` = an approval that started in a fresh context
/// (ask-dock.json).
public struct ThreadPlanRow: Equatable, Sendable, Identifiable {
    public let id: String
    public let sessionId: String
    public let plan: String
    /// EXP-1280: the choice questions, the inline surface.
    public let questions: [ThreadPlanQuestion]
    public let surface: CardSurface?
    public let decision: String?
    public let fresh: Bool
    /// EXP-1280: the picked labels per question, the note.
    public let answers: [String: [String]]?
    public let feedback: String?
    /// EXP-1280: the surface's data model as the person answered it.
    public let surfaceData: CardJSON?

    public init(
        id: String,
        sessionId: String,
        plan: String,
        questions: [ThreadPlanQuestion] = [],
        surface: CardSurface? = nil,
        decision: String? = nil,
        fresh: Bool = false,
        answers: [String: [String]]? = nil,
        feedback: String? = nil,
        surfaceData: CardJSON? = nil
    ) {
        self.id = id
        self.sessionId = sessionId
        self.plan = plan
        self.questions = questions
        self.surface = surface
        self.decision = decision
        self.fresh = fresh
        self.answers = answers
        self.feedback = feedback
        self.surfaceData = surfaceData
    }
}

/// EXP-1323: one suggested task: `open` (× · Options · Do it here · Start),
/// `started` (the child's card once synced), `here` (sent to the posting
/// session), `filed` (the issue it became). Dismissed items drop.
public struct SuggestionRow: Equatable, Sendable, Identifiable {
    public enum State: String, Equatable, Sendable {
        case open
        case started
        case here
        case filed
    }

    public let id: String
    public let title: String
    public let text: String
    public let state: State
    public let card: SessionCard?
    public let issueId: String?
    public let identifier: String?

    public init(
        id: String,
        title: String,
        text: String,
        state: State,
        card: SessionCard?,
        issueId: String?,
        identifier: String?
    ) {
        self.id = id
        self.title = title
        self.text = text
        self.state = state
        self.card = card
        self.issueId = issueId
        self.identifier = identifier
    }
}

/// The composer below a thread: THE session composer box.
public struct ThreadComposer: Equatable, Sendable {
    /// `sessions.send({sessionId})` target (the root).
    public let sessionId: String
    public let placeholder: String

    public init(sessionId: String, placeholder: String) {
        self.sessionId = sessionId
        self.placeholder = placeholder
    }
}

public struct SessionThreadView: Equatable, Sendable {
    public let rootId: String
    public let title: String
    /// A lock glyph beside the title (`!root.shared`).
    public let isPrivate: Bool
    /// The focused session (nil = the thread itself).
    public let focusId: String?
    public let tabs: [ThreadTab]
    public let rows: [ThreadRow]
    public let composer: ThreadComposer
    public let live: Bool
    /// EXP-1323: the live tail (Tasks card + status line) reads this
    /// session's relay state; nil once the root ended.
    public let liveSessionId: String?
    public let busy: Bool
    public let needsYou: Bool
    /// NEW-6 (N-1): a thread with ONE session (the root alone) shows that
    /// session's raw work as the main thing: its id, and no Conversation
    /// tab. Nil once a second session exists.
    public let mainSessionId: String?

    public init(
        rootId: String,
        title: String,
        isPrivate: Bool,
        focusId: String?,
        tabs: [ThreadTab],
        rows: [ThreadRow],
        composer: ThreadComposer,
        live: Bool,
        liveSessionId: String? = nil,
        busy: Bool,
        needsYou: Bool,
        mainSessionId: String? = nil
    ) {
        self.liveSessionId = liveSessionId
        self.mainSessionId = mainSessionId
        self.rootId = rootId
        self.title = title
        self.isPrivate = isPrivate
        self.focusId = focusId
        self.tabs = tabs
        self.rows = rows
        self.composer = composer
        self.live = live
        self.busy = busy
        self.needsYou = needsYou
    }
}

public struct ThreadsListRow: Equatable, Sendable, Identifiable {
    public let sessionId: String
    public let teamId: String
    /// EXP-1323: 0 = the thread (opens the thread), >0 = a nested child
    /// (opens that session); `parentId` nil on a root.
    public let depth: Int
    public let parentId: String?
    public let rootId: String
    public let title: String
    public let busy: Bool
    public let needsYou: Bool
    /// The tree's newest `updatedAt` (wire text).
    public let activityAt: String
    /// EXP-1316: the ROOT's device (the row's trailing device glyph,
    /// resolved against the synced devices rows); nil = none.
    public let deviceId: String?
    /// EXP-1323: bold title (a root: the tree's newest agent message is
    /// newer than the viewer's read of the root; a child: its own).
    public let unread: Bool

    public var id: String { sessionId }

    public init(
        sessionId: String,
        teamId: String,
        depth: Int = 0,
        parentId: String? = nil,
        rootId: String? = nil,
        title: String,
        busy: Bool,
        needsYou: Bool,
        unread: Bool = false,
        activityAt: String,
        deviceId: String? = nil
    ) {
        self.deviceId = deviceId
        self.depth = depth
        self.parentId = parentId
        self.rootId = rootId ?? sessionId
        self.unread = unread
        self.sessionId = sessionId
        self.teamId = teamId
        self.title = title
        self.busy = busy
        self.needsYou = needsYou
        self.activityAt = activityAt
    }
}

public struct ThreadsListGroup: Equatable, Sendable, Identifiable {
    public enum Key: String, Equatable, Sendable, CaseIterable {
        case needsYou = "needs_you"
        case running
        case recent

        /// Fixture `copy.groups`.
        public var title: String {
            switch self {
            case .needsYou: "Needs you"
            case .running: "Running"
            case .recent: "Recent"
            }
        }
    }

    public let key: Key
    public let title: String
    public let rows: [ThreadsListRow]

    public var id: String { key.rawValue }

    public init(key: Key, title: String, rows: [ThreadsListRow]) {
        self.key = key
        self.title = title
        self.rows = rows
    }
}

/// EXP-1309: a Threads row's ONE quiet trailing word (web
/// `THREAD_ROW_WORDS`); `SessionTree.threadRowWord` picks it.
public enum ThreadRowWord: String, Equatable, Sendable, CaseIterable {
    case stack
    case readyToMerge
    case merged
    case question
    /// An agent row waiting on a person.
    case needsInput

    /// EXP-1339: `merged` has no text (it only turns the lead violet).
    public var text: String? {
        switch self {
        case .readyToMerge: "ready to merge"
        case .stack: "stack"
        case .merged: nil
        case .question: "question"
        case .needsInput: "needs input"
        }
    }
}

/// A pull request as far as the stack test reads it.
public struct StackPr: Equatable, Sendable {
    public let id: String
    public let state: String
    public let basePullRequestId: String?

    public init(id: String, state: String, basePullRequestId: String?) {
        self.id = id
        self.state = state
        self.basePullRequestId = basePullRequestId
    }

    public init(_ pr: PullRequestEntity) {
        self.init(id: pr.id, state: pr.state, basePullRequestId: pr.basePullRequestId)
    }
}

/// NEW-2: an `agents` definition row as the Agents band needs it.
public struct AgentLike: Equatable, Sendable, Identifiable {
    public let id: String
    public let teamId: String
    public let name: String
    public let icon: String?
    public let color: String?
    public let sortOrder: Double

    public init(id: String, teamId: String, name: String, icon: String?, color: String?, sortOrder: Double) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.color = color
        self.sortOrder = sortOrder
    }

    public init(_ agent: AgentDefinitionEntity) {
        self.init(
            id: agent.id, teamId: agent.teamId, name: agent.name,
            icon: agent.icon, color: agent.color, sortOrder: agent.sortOrder
        )
    }
}

/// One row of the Agents band (icon + color like a board row): an agent
/// DEFINITION (NEW-2); opening it opens the viewer's conversation with it.
public struct AgentRow: Equatable, Sendable, Identifiable {
    public let agentId: String
    public let teamId: String
    public let name: String
    /// Icon NAME (contract `boardIconValues`).
    public let icon: String
    /// `#rrggbb`.
    public let color: String
    public let busy: Bool
    public let needsYou: Bool

    public var id: String { agentId }

    public init(
        agentId: String,
        teamId: String,
        name: String,
        icon: String,
        color: String,
        busy: Bool,
        needsYou: Bool
    ) {
        self.agentId = agentId
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.color = color
        self.busy = busy
        self.needsYou = needsYou
    }
}

// MARK: - The rules

public enum SessionTree {
    /// fixtures/session-card.json `copy` + `testIds`.
    public enum CardCopy {
        public static let testId = "session-card"
        /// NEW-1: `copy.blockedKinds` — the state word per `blocked.kind`
        /// (contract codingSessionBlocked.kinds); an unknown kind reads
        /// `SessionState.blocked.word`.
        public static let blockedKinds: [String: String] = [
            "rate_limit": "Usage limit",
            "device_offline": "Device offline",
            "unclaimed": "Not picked up",
            "stuck": "Stuck",
        ]
    }

    /// fixtures/session-thread.json `copy` + `testIds` (titles: `SessionTitle`).
    public enum ThreadCopy {
        public static let untitled = "Untitled"
        public static let placeholder = "Type / for commands"
        public static let conversation = "Conversation"
        public static let guide = "Guide"
        /// `{issue}` = the identifier, `{pr}` = `#<number>`, `{session}` =
        /// the named session's card name (NEW-1).
        public static let events: [String: String] = [
            "issue_linked": "Linked {issue}",
            "issue_filed": "Filed {issue}",
            "issue_unlinked": "Unlinked {issue}",
            "pr_opened": "Opened PR {pr}",
            "pr_merged": "Merged PR {pr}",
            "pr_closed": "Closed PR {pr}",
            "session_ended": "Session ended",
            "session_claimed": "{session} started",
            "session_blocked": "{session} hit a usage limit",
            "session_unclaimed": "{session} was not picked up",
            "session_stuck": "{session} looks stuck",
            "device_offline": "{session} paused: device offline",
            // NEW-2: an agent's trigger fired ({name} = the event name, else
            // the schedule's cron).
            "trigger_fired": "Trigger fired: {name}",
        ]
        public static let tabsTestId = "thread-tabs"
        /// EXP-1334: a zone's "+N" chip reads out as web's
        /// `THREAD_STRIP_MORE_COPY` (`2 more issues`, `1 more session`).
        public static func more(_ count: Int, zone: String) -> String {
            let noun = zone == "issues" ? "issue" : "session"
            return "\(count) more \(noun)\(count == 1 ? "" : "s")"
        }
        public static let conversationTestId = "thread-conversation"
        public static let composerTestId = "thread-composer"
        /// EXP-1306: the Guide's stack band.
        public static let stack = "Stack"
        public static let guideStackTestId = "thread-guide-stack"
        public static let guideMergeTestId = "thread-guide-merge"
        /// EXP-1307: the header's `…` on the Guide (`guideMenuEntries`).
        public static let guideMenuTestId = "thread-guide-menu"
        /// EXP-1323: `copy.report`, `{session}` = the card name.
        public static let report = "Report from {session}"
    }

    /// EXP-1323: fixtures/session-thread.json `copy.suggestions`.
    public enum SuggestionsCopy {
        public static let one = "1 suggested task"
        /// `{count}`.
        public static let many = "{count} suggested tasks"
        public static let started = "{count} started"
        public static let startAll = "Start all"
        public static let start = "Start"
        public static let here = "Do it here"
        public static let options = "Options"
        public static let startOn = "Start on…"
        public static let startWith = "Start with…"
        public static let file = "File as issue"
        public static let dismiss = "Dismiss"
        public static let sentHere = "Sent here"
        /// `{issue}`.
        public static let filed = "Filed {issue}"
    }

    /// EXP-1211: fixtures/session-thread.json `copy.replies` + `testIds.replies`.
    public enum RepliesCopy {
        public static let label = "Quick replies"
        public static let dismiss = "Dismiss quick replies"
        public static let testId = "thread-replies"
    }

    /// fixtures/threads-list.json `copy` + `agentDefaults` + `testIds`
    /// (group titles = `ThreadsListGroup.Key.title`, words =
    /// `ThreadRowWord.text`).
    public enum ListCopy {
        public static let title = "Threads"
        /// The rail's one band (web + IDE).
        public static let rail = "Active"
        public static let agents = "Agents"
        public static let empty = "No sessions yet"
        public static let agentDefaultIcon = "bot"
        public static let agentDefaultColor = "#6366f1"
        public static let listTestId = "threads-list"
        public static let rowTestId = "threads-row"
        public static let agentRowTestId = "agents-row"
    }

    /// The work card's elapsed ladder: `37s`, `2m 04s`, `1h 03m`.
    public static func formatElapsed(ms durationMs: Double) -> String {
        let total = max(0, Int((durationMs / 1000).rounded(.down)))
        let hours = total / 3600
        let minutes = (total % 3600) / 60
        let seconds = total % 60
        func pad(_ value: Int) -> String { value < 10 ? "0\(value)" : "\(value)" }
        if hours > 0 { return "\(hours)h \(pad(minutes))m" }
        if minutes > 0 { return "\(minutes)m \(pad(seconds))s" }
        return "\(seconds)s"
    }

    public static func sessionState(_ session: TreeSession) -> SessionState {
        if session.status == "ended" { return .ended }
        if session.needsInput { return .needsYou }
        if session.blocked != nil { return .blocked }
        if session.status == "queued" { return .queued }
        if session.busy { return .working }
        return .waiting
    }

    /// EXP-1312: a thread row's lead outside the Threads tab (Home's Running
    /// band) ×4: needs you beats working, else the bare mark
    /// (`threads-list.json` `markCases`).
    public static func threadRowMarkState(needsYou: Bool, busy: Bool) -> SessionState {
        if needsYou { return .needsYou }
        if busy { return .working }
        return .waiting
    }

    /// NEW-41 ×4: the PR glyph a thread row leads with once
    /// `threadRowMarkState` is `.waiting`, off the ROOT's PR states (never the
    /// word, so a stacked open PR still leads emerald): any open → `.open`,
    /// else any merged → `.merged`, else none (`threads-list.json`
    /// `leadCases`).
    public enum ThreadRowLeadPr: String { case open, merged }

    public static func threadRowLeadPr(prStates: [String]) -> ThreadRowLeadPr? {
        if prStates.contains(DomainContract.prStateOpen) { return .open }
        if prStates.contains(DomainContract.prStateMerged) { return .merged }
        return nil
    }

    /// EXP-1316: a session chip's lead: that session's row mark, `.ended`
    /// once it ended.
    public static func sessionTabMarkState(_ session: TreeSession) -> SessionState {
        if session.status == "ended" { return .ended }
        return threadRowMarkState(needsYou: session.needsInput, busy: session.busy)
    }

    /// Sessions of one tree in creation order (ties by id).
    public static func treeOrder(_ sessions: [TreeSession]) -> [TreeSession] {
        sessions.sorted { a, b in
            let ma = ms(a.createdAt) ?? 0, mb = ms(b.createdAt) ?? 0
            if ma != mb { return ma < mb }
            return a.id < b.id
        }
    }

    /// THE session card. `title` = its `SessionTitle.title` (EXP-1341,
    /// fixtures/session-title.json; never `Session N`).
    public static func sessionCard(_ session: TreeSession, title: String, now: Date) -> SessionCard {
        let state = sessionState(session)
        let start = ms(session.startedAt)
        let end = state == .ended ? (ms(session.endedAt) ?? ms(session.updatedAt)) : ms(now)
        var elapsed: String?
        if let start, let end, state != .queued { elapsed = formatElapsed(ms: end - start) }
        return SessionCard(
            sessionId: session.id,
            icon: state.icon,
            name: title,
            kindWord: sessionKindWord(session, state: state),
            state: state,
            tone: state.tone,
            elapsed: elapsed,
            running: state == .working,
            testId: CardCopy.testId
        )
    }

    /// NEW-1: the card's state word; a blocked session names WHY
    /// (`CardCopy.blockedKinds[blocked.kind]`; an unknown or absent kind =
    /// `SessionState.blocked.word`). Web `sessionKindWord`.
    public static func sessionKindWord(_ session: TreeSession, state: SessionState) -> String {
        if state == .blocked, let kind = session.blocked?.kind, let word = CardCopy.blockedKinds[kind] {
            return word
        }
        return state.word
    }

    /// THE display title of one tree session off the tree rows
    /// (`SessionTitle.title`).
    public static func sessionTitle(
        _ session: TreeSession,
        sessionIssues: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [TreeMessage]
    ) -> String {
        SessionTitle.title(session, sessionIssues: sessionIssues, issues: issues, messages: messages)
    }

    /// The thread screen: the header tabs, the root's conversation as rows
    /// (its messages + its children's question/plan cards, oldest first; no
    /// root card, EXP-1334) and the composer target (the root). `focus` = the
    /// open session (echoed as `focusId`; it no longer changes the chips).
    public static func buildSessionThread(
        root: TreeSession,
        sessions: [TreeSession],
        sessionIssues: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [TreeMessage],
        pullRequests: [TreePullRequest],
        now: Date,
        focus: String? = nil
    ) -> SessionThreadView {
        var tree = treeOrder(sessions.filter { $0.rootId == root.id || $0.id == root.id })
        if !tree.contains(where: { $0.id == root.id }) { tree.insert(root, at: 0) }
        var byId: [String: TreeSession] = [:]
        for session in sessions { byId[session.id] = session }
        byId[root.id] = root
        let titles = SessionTitle.titles(Array(byId.values), sessionIssues: sessionIssues, issues: issues, messages: messages)
        let linked = treeIssues(tree, sessionIssues: sessionIssues, issues: issues)
        var issueMap: [String: TreeIssue] = [:]
        for issue in issues { issueMap[issue.id] = issue }
        var prMap: [String: TreePullRequest] = [:]
        for pr in pullRequests { prMap[pr.id] = pr }
        let own = messages
            .filter { $0.sessionId == root.id }
            .sorted { a, b in
                let ma = ms(a.createdAt) ?? 0, mb = ms(b.createdAt) ?? 0
                if ma != mb { return ma < mb }
                return a.id < b.id
            }
        let title = titles[root.id] ?? SessionTitle.Copy.untitled
        let focused = focus.flatMap { id in tree.first { $0.id == id } }
        // EXP-1334: the conversation IS the dispatcher (no chip of its own);
        // the chips = its DIRECT children, every deeper session folded under
        // its top-level ancestor (the chip's dropdown).
        func nameOf(_ session: TreeSession) -> String {
            sessionCard(session, title: titles[session.id] ?? "", now: now).name
        }
        let treeIds = Set(tree.map(\.id))
        let sessionTabs: [ThreadTab] = foldForest(
            tree.filter { $0.id != root.id },
            id: \.id,
            foldParent: { session in
                guard let parent = session.parentId, parent != root.id, treeIds.contains(parent) else { return nil }
                return parent
            }
        ).map { top, nested in
            .session(
                sessionId: top.id,
                name: nameOf(top),
                markState: sessionTabMarkState(top),
                nested: nested.map {
                    ThreadTabNestedSession(
                        sessionId: $0.item.id, name: nameOf($0.item),
                        markState: sessionTabMarkState($0.item), depth: $0.depth
                    )
                }
            )
        }
        let linkedIds = Set(linked.map(\.id))
        let issueTabs: [ThreadTab] = foldForest(
            linked,
            id: \.id,
            foldParent: { issueFoldParent($0, linked: linkedIds, all: issueMap) }
        ).map { top, nested in
            .issue(
                issueId: top.id,
                identifier: top.identifier,
                nested: nested.map {
                    ThreadTabNestedIssue(issueId: $0.item.id, identifier: $0.item.identifier, depth: $0.depth)
                }
            )
        }

        // EXP-1334: no root card; a CHILD's own question / plan card surfaces
        // in the Conversation (answered here, straight back to that child).
        let asks = messages.filter { message in
            guard message.sessionId != root.id, treeIds.contains(message.sessionId),
                  let kind = message.card?.kind else { return false }
            return kind == "question" || kind == "plan"
        }
        let conversation = (own + asks).sorted { a, b in
            let ma = ms(a.createdAt) ?? 0, mb = ms(b.createdAt) ?? 0
            if ma != mb { return ma < mb }
            return a.id < b.id
        }
        var rows: [ThreadRow] = []
        for message in conversation {
            guard let card = message.card else {
                let author = message.authorKind == "user" || message.authorKind == "agent"
                    ? message.authorKind
                    : "system"
                if message.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { continue }
                rows.append(.message(
                    id: message.id,
                    author: author,
                    authorId: message.authorId,
                    body: message.body,
                    queued: author == "user" && message.consumedAt == nil,
                    authorSessionId: message.authorSessionId
                ))
                continue
            }
            switch card.kind {
            case "session", "thread_started":
                guard let targetId = card.sessionId, let target = byId[targetId] else { break }
                rows.append(.session(
                    id: message.id,
                    card: sessionCard(target, title: titles[target.id] ?? "", now: now)
                ))
            case "question", "plan":
                if let row = ThreadRow.askCard(message) { rows.append(row) }
            case "subagent":
                rows.append(.subagent(id: message.id, title: card.title ?? "", status: card.status ?? ""))
            case "picture":
                guard let attachmentId = card.attachmentId else { break }
                rows.append(.picture(
                    id: message.id,
                    attachmentId: attachmentId,
                    width: card.width,
                    height: card.height,
                    caption: card.caption
                ))
            case "report":
                guard let targetId = card.sessionId, let target = byId[targetId] else { break }
                rows.append(.report(
                    id: message.id,
                    sessionId: target.id,
                    text: ThreadCopy.report.replacingOccurrences(
                        of: "{session}",
                        with: sessionCard(target, title: titles[target.id] ?? "", now: now).name
                    ),
                    summary: card.summary ?? message.body
                ))
            case "suggestions":
                if let row = suggestionsRow(message, card: card, byId: byId, titles: titles, issues: issueMap, now: now) {
                    rows.append(row)
                }
            case "event":
                let text = eventText(card, issues: issueMap, pullRequests: prMap) { id in
                    byId[id].map { sessionCard($0, title: titles[$0.id] ?? "", now: now).name }
                }
                if let text {
                    rows.append(.event(id: message.id, event: card.event ?? "", text: text))
                }
            // NEW-1: a child's ask mirrored into its parent (`ask`): the
            // child's own card already surfaces in the Conversation, so the
            // mirror draws nothing here.
            case "ask":
                break
            default:
                break
            }
        }

        let guideDot = (root.guide ?? []).contains { entry in
            !(entry.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || entry.label != nil
        }
        let live = tree.filter { $0.status != "ended" }
        // NEW-6 (N-1): the root alone = the thread IS that session (the
        // Conversation tab opens its raw work, `mainSessionId`); NEW-46: the
        // tab itself is always in the strip.
        let single = tree.allSatisfy { $0.id == root.id }
        return SessionThreadView(
            rootId: root.id,
            title: title,
            isPrivate: !root.shared,
            focusId: focused?.id,
            tabs: [.add, .conversation] + issueTabs + sessionTabs + [.guide(dot: guideDot)],
            rows: rows,
            composer: ThreadComposer(sessionId: root.id, placeholder: ThreadCopy.placeholder),
            live: !live.isEmpty,
            liveSessionId: root.status == "ended" ? nil : root.id,
            busy: live.contains { $0.busy },
            needsYou: live.contains { $0.needsInput },
            mainSessionId: single ? root.id : nil
        )
    }

    /// Depth-first walk of a forest given each node's FOLD parent (nil = a
    /// top-level node), children in the given order.
    static func foldForest<T>(
        _ items: [T],
        id: (T) -> String,
        foldParent: (T) -> String?
    ) -> [(top: T, nested: [(item: T, depth: Int)])] {
        var children: [String: [T]] = [:]
        var tops: [T] = []
        for item in items {
            if let parent = foldParent(item) {
                children[parent, default: []].append(item)
            } else {
                tops.append(item)
            }
        }
        var seen: Set<String> = []
        func fold(_ top: T) -> (top: T, nested: [(item: T, depth: Int)]) {
            var nested: [(item: T, depth: Int)] = []
            seen.insert(id(top))
            func walk(_ parentId: String, _ depth: Int) {
                for child in children[parentId] ?? [] {
                    let childId = id(child)
                    if seen.contains(childId) { continue }
                    seen.insert(childId)
                    nested.append((child, depth))
                    walk(childId, depth + 1)
                }
            }
            walk(id(top), 1)
            return (top, nested)
        }
        var out = tops.map(fold)
        // A fold-parent cycle (a <-> b) reaches no top: its first unreached
        // item (input order) becomes a top-level chip.
        for item in items where !seen.contains(id(item)) { out.append(fold(item)) }
        return out
    }

    /// EXP-1334: a linked issue's fold parent = its nearest LINKED ancestor
    /// (an unlinked parent is skipped; a cycle or none = top-level).
    static func issueFoldParent(
        _ issue: TreeIssue,
        linked: Set<String>,
        all: [String: TreeIssue]
    ) -> String? {
        var seen: Set<String> = [issue.id]
        var parent = issue.parentId
        while let current = parent, !seen.contains(current) {
            if linked.contains(current) { return current }
            seen.insert(current)
            parent = all[current]?.parentId
        }
        return nil
    }

    // MARK: - EXP-1334: the thread strip never scrolls

    /// Which chips get drawn, in order; the rest collapse into one "+N" (a
    /// port of the desktop's `partition_tabs`): chips pack in order until the
    /// next would not fit; the overflow chip's width is reserved only once
    /// something overflows; the ACTIVE chip is always visible; one chip
    /// always survives.
    public static func partitionTabs(
        _ widths: [Double],
        available: Double,
        gap: Double,
        overflowW: Double,
        activeIndex: Int?
    ) -> [Int] {
        let count = widths.count
        let room = max(available, 0)
        let total = widths.reduce(0, +) + gap * Double(max(count - 1, 0))
        if total <= room { return Array(0..<count) }
        let budget = max(room - overflowW - gap, 0)
        var visible: [Int] = []
        var used = activeIndex.map { widths[$0] } ?? 0
        var chips = activeIndex == nil ? 0 : 1
        for index in 0..<count {
            if index == activeIndex {
                visible.append(index)
                continue
            }
            let next = used + widths[index] + (chips > 0 ? gap : 0)
            if next > budget && chips > 0 { break }
            visible.append(index)
            used = next
            chips += 1
        }
        if let activeIndex, !visible.contains(activeIndex) { visible.append(activeIndex) }
        if visible.isEmpty && count > 0 { visible.append(0) }
        return visible
    }

    /// EXP-1334: the strip between Conversation and Guide: issues then
    /// sessions. Tiers: everything full → every issue short → fold (the
    /// sessions keep their OWN room, at least half; each zone packs with
    /// `partitionTabs` and its own "+N").
    public static func partitionThreadStrip(
        issues: [(full: Double, short: Double)],
        sessions: [Double],
        available: Double,
        gap: Double,
        overflowW: Double,
        activeIssue: Int?,
        activeSession: Int?
    ) -> ThreadStripLayout {
        let room = max(available, 0)
        func span(_ widths: [Double]) -> Double {
            widths.isEmpty ? 0 : widths.reduce(0, +) + gap * Double(widths.count - 1)
        }
        let sep = !issues.isEmpty && !sessions.isEmpty ? gap : 0
        let sessionSpan = span(sessions)
        let full = issues.map(\.full)
        let short = issues.map(\.short)
        if span(full) + sep + sessionSpan <= room {
            return ThreadStripLayout(tier: .full, issues: Array(0..<issues.count), sessions: Array(0..<sessions.count))
        }
        if span(short) + sep + sessionSpan <= room {
            return ThreadStripLayout(tier: .short, issues: Array(0..<issues.count), sessions: Array(0..<sessions.count))
        }
        func zone(_ widths: [Double], _ budget: Double, _ active: Int?) -> (shown: [Int], used: Double) {
            let shown = partitionTabs(widths, available: budget, gap: gap, overflowW: overflowW, activeIndex: active)
            let used = span(shown.map { widths[$0] }) + (shown.count < widths.count ? gap + overflowW : 0)
            return (shown, used)
        }
        let sessionBudget = issues.isEmpty
            ? room
            : min(sessionSpan, max(room / 2, room - span(short) - sep))
        let placed = zone(sessions, sessionBudget, activeSession)
        let issueBudget = room - (sessions.isEmpty ? 0 : placed.used + sep)
        let issueZone = zone(short, issueBudget, activeIssue)
        return ThreadStripLayout(tier: .fold, issues: issueZone.shown, sessions: placed.shown)
    }

    private static func suggestionsRow(
        _ message: TreeMessage,
        card: TreeMessageCard,
        byId: [String: TreeSession],
        titles: [String: String],
        issues: [String: TreeIssue],
        now: Date
    ) -> ThreadRow? {
        guard let raw = card.items else { return nil }
        let items: [SuggestionRow] = raw.filter { !$0.dismissed }.map { item in
            let target = item.sessionId.flatMap { byId[$0] }
            let state: SuggestionRow.State = item.sessionId != nil
                ? .started
                : item.here ? .here : item.issueId != nil ? .filed : .open
            return SuggestionRow(
                id: item.id,
                title: item.title,
                text: item.text,
                state: state,
                card: target.map { sessionCard($0, title: titles[$0.id] ?? "", now: now) },
                issueId: item.issueId,
                identifier: item.issueId.flatMap { issues[$0]?.identifier }
            )
        }
        guard !items.isEmpty else { return nil }
        let started = items.filter { $0.state == .started }.count
        let count = items.count == 1
            ? SuggestionsCopy.one
            : SuggestionsCopy.many.replacingOccurrences(of: "{count}", with: String(items.count))
        return .suggestions(
            id: message.id,
            sessionId: message.sessionId,
            header: started > 0
                ? "\(count) · \(SuggestionsCopy.started.replacingOccurrences(of: "{count}", with: String(started)))"
                : count,
            startAll: items.filter { $0.state == .open }.count > 1,
            items: items
        )
    }

    /// The Threads list: the roots of kind `thread` in scope (`teamId` = the
    /// ACTIVE team; nil = every team, phones band rows per team), grouped
    /// Needs you · Running · Recent, each by the tree's newest `updatedAt`
    /// desc. Empty groups are omitted.
    public static func partitionThreads(
        sessions: [TreeSession],
        sessionIssues: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [TreeMessage],
        teamId: String?,
        reads: [TreeSessionRead] = [],
        now: String? = nil
    ) -> [ThreadsListGroup] {
        // EXP-1323: unread = the newest agent-authored message (a root: any
        // session of its tree; a child: its own) is newer than the viewer's
        // read of that session.
        var readAt: [String: Double] = [:]
        for read in reads { readAt[read.sessionId] = ms(read.readAt) ?? 0 }
        var rootOf: [String: String] = [:]
        for session in sessions { rootOf[session.id] = session.rootId }
        var newestAgent: [String: Double] = [:]
        var newestAgentInTree: [String: Double] = [:]
        for message in messages where message.authorKind == "agent" {
            let at = ms(message.createdAt) ?? 0
            newestAgent[message.sessionId] = max(newestAgent[message.sessionId] ?? 0, at)
            if let rootId = rootOf[message.sessionId] {
                newestAgentInTree[rootId] = max(newestAgentInTree[rootId] ?? 0, at)
            }
        }
        func unread(_ newest: Double?, _ sessionId: String) -> Bool {
            guard let newest else { return false }
            return newest > (readAt[sessionId] ?? -1)
        }
        let roots = sessions.filter {
            // NEW-2: an agent's conversation (a root with `agentId`) is no thread;
            // VAPP-110: neither is an app's editor session (`appId`).
            $0.parentId == nil && ($0.agentId ?? "").isEmpty && ($0.appId ?? "").isEmpty
                && (teamId == nil || $0.teamId == teamId)
        }
        var byRoot: [String: [TreeSession]] = [:]
        for session in sessions { byRoot[session.rootId, default: []].append(session) }
        // EXP-1358: the title lookups and every `updatedAt` parsed ONCE —
        // this ran per row (and per sort comparison) on the main actor.
        let titleIndex = SessionTitle.index(sessionIssues: sessionIssues, issues: issues, messages: messages)
        var updatedMs: [String: Double] = [:]
        for session in sessions { updatedMs[session.id] = ms(session.updatedAt) ?? 0 }
        var buckets: [ThreadsListGroup.Key: [(row: ThreadsListRow, children: [ThreadsListRow], activityMs: Double)]] = [:]
        for root in roots {
            let tree = byRoot[root.id] ?? [root]
            let live = tree.filter { $0.status != "ended" }
            let needsYou = live.contains { $0.needsInput }
            let running = live.contains { $0.status == "queued" || $0.status == "running" }
            var activity = root.updatedAt
            var activityMs = ms(root.updatedAt) ?? 0
            for session in tree where (updatedMs[session.id] ?? 0) > activityMs {
                activity = session.updatedAt
                activityMs = updatedMs[session.id] ?? 0
            }
            let title = SessionTitle.title(SessionTitle.Session(root), index: titleIndex)
            let row = ThreadsListRow(
                sessionId: root.id,
                teamId: root.teamId,
                depth: 0,
                parentId: nil,
                rootId: root.id,
                title: title,
                busy: live.contains { $0.busy },
                needsYou: needsYou,
                unread: unread(newestAgentInTree[root.id], root.id),
                activityAt: activity,
                deviceId: root.deviceId
            )
            let key: ThreadsListGroup.Key = needsYou ? .needsYou : running ? .running : .recent
            // A live tree nests its children (depth-first, creation order);
            // Recent collapses to the root.
            var children: [ThreadsListRow] = []
            if key != .recent {
                let ordered = treeOrder(tree)
                func walk(_ parentId: String, _ depth: Int) {
                    for child in ordered where child.parentId == parentId {
                        let ended = child.status == "ended"
                        children.append(ThreadsListRow(
                            sessionId: child.id,
                            teamId: child.teamId,
                            depth: depth,
                            parentId: parentId,
                            rootId: root.id,
                            title: SessionTitle.title(SessionTitle.Session(child), index: titleIndex),
                            busy: !ended && child.busy,
                            needsYou: !ended && child.needsInput,
                            unread: unread(newestAgent[child.id], child.id),
                            activityAt: child.updatedAt,
                            deviceId: child.deviceId
                        ))
                        walk(child.id, depth + 1)
                    }
                }
                walk(root.id, 1)
            }
            buckets[key, default: []].append((row, children, activityMs))
        }
        // NEW-1: Recent = every tree active in the last `recent.hours`,
        // filled up to the last `recent.max` by activity (no clock = the
        // last `recent.max` only). Ended trees stay openable.
        let at = ms(now)
        return ThreadsListGroup.Key.allCases.compactMap { key in
            var entries = (buckets[key] ?? []).sorted { a, b in
                if a.activityMs != b.activityMs { return a.activityMs > b.activityMs }
                return a.row.sessionId < b.row.sessionId
            }
            if key == .recent {
                entries = entries.enumerated().filter { index, entry in
                    index < ListRules.recentMax
                        || (at.map { $0 - entry.activityMs <= ListRules.recentHours * 60 * 60 * 1000 } ?? false)
                }.map(\.element)
            }
            let rows = entries.flatMap { [$0.row] + $0.children }
            return rows.isEmpty ? nil : ThreadsListGroup(key: key, title: key.title, rows: rows)
        }
    }

    /// NEW-1: fixtures/threads-list.json `recent` + `treeState`.
    public enum ListRules {
        public static let recentHours: Double = 24
        public static let recentMax = 20
        public static let staleHours: Double = 24
    }

    /// NEW-1 (plan 3.13): a tree's derived state (threads-list.json
    /// `treeState`).
    public enum TreeState: String, Equatable, Sendable, CaseIterable {
        case needsYou = "needs_you"
        case running
        case done
        case stale
        case idle
    }

    /// NEW-1: `needs_you` = a live session waits on a person (a card, a
    /// usage wall or a watchdog flag all set `needsInput`); `running` = any
    /// session queued or running; `done` = every session ended and every PR
    /// of the thread merged (none = never done); `stale` = nothing touched
    /// for `staleHours`; else `idle`.
    public static func treeState(
        _ tree: [TreeSession],
        pullRequests: [TreePullRequest],
        now: String?
    ) -> TreeState {
        let live = tree.filter { $0.status != "ended" }
        if live.contains(where: { $0.needsInput }) { return .needsYou }
        if live.contains(where: { $0.status == "queued" || $0.status == "running" }) { return .running }
        if live.isEmpty, !pullRequests.isEmpty, pullRequests.allSatisfy({ $0.state == "merged" }) {
            return .done
        }
        let newest = tree.reduce(0.0) { Swift.max($0, ms($1.updatedAt) ?? 0) }
        if let at = ms(now), at - newest > ListRules.staleHours * 60 * 60 * 1000 { return .stale }
        return .idle
    }

    /// EXP-1323: the Threads nav dot = any thread (depth 0) row is unread.
    public static func threadsUnread(_ groups: [ThreadsListGroup]) -> Bool {
        groups.contains { group in group.rows.contains { $0.depth == 0 && $0.unread } }
    }

    /// The Agents band: one row per `agents` definition of the team (nil =
    /// every team, NEW-2) by `sortOrder`, then name, then id. The spinner /
    /// needs-you dot read the VIEWER's own conversation session with the
    /// agent (a root with `agentId` and `userId == me`); none = a quiet row.
    public static func listAgents(
        agents: [AgentLike], sessions: [TreeSession], teamId: String?, me: String?
    ) -> [AgentRow] {
        var mine: [String: TreeSession] = [:]
        for session in sessions where session.parentId == nil && session.userId == me {
            guard let agentId = session.agentId, !agentId.isEmpty else { continue }
            mine[agentId] = session
        }
        return agents
            .filter { teamId == nil || $0.teamId == teamId }
            .sorted { a, b in
                if a.sortOrder != b.sortOrder { return a.sortOrder < b.sortOrder }
                let byName = a.name.localizedCompare(b.name)
                if byName != .orderedSame { return byName == .orderedAscending }
                return a.id < b.id
            }
            .map { agent in
                let conversation = mine[agent.id]
                let live = conversation.map { $0.status != "ended" } ?? false
                return AgentRow(
                    agentId: agent.id,
                    teamId: agent.teamId,
                    name: agent.name,
                    icon: agent.icon ?? ListCopy.agentDefaultIcon,
                    color: agent.color ?? ListCopy.agentDefaultColor,
                    busy: live && conversation?.busy == true,
                    needsYou: live && conversation?.needsInput == true
                )
            }
    }

    /// The caller's own working sessions counted per team — the board
    /// switcher SHEET's per-team dots (EXP-1075/EXP-1210). A session counts
    /// while it is queued or running; `needsInput` while any own non-ended
    /// session waits on a person. No resolved user owns nothing.
    public struct TeamLiveSessions: Equatable, Sendable {
        public var count: Int
        public var needsInput: Bool

        public init(count: Int = 0, needsInput: Bool = false) {
            self.count = count
            self.needsInput = needsInput
        }
    }

    public static func liveByTeam(_ sessions: [TreeSession], userId: String?) -> [String: TeamLiveSessions] {
        guard let userId, !userId.isEmpty else { return [:] }
        var byTeam: [String: TeamLiveSessions] = [:]
        for session in sessions where session.userId == userId && session.status != "ended" {
            let working = session.status == "queued" || session.status == "running"
            guard working || session.needsInput else { continue }
            var entry = byTeam[session.teamId] ?? TeamLiveSessions()
            if working { entry.count += 1 }
            if session.needsInput { entry.needsInput = true }
            byTeam[session.teamId] = entry
        }
        return byTeam
    }

    /// EXP-1306: the ONE pull request a thread's Guide speaks for (its stack
    /// rail + the bottom Merge capsule): the newest open one, else the newest
    /// merged one (createdAt, ties by the higher number); a closed PR never
    /// counts.
    public static func guidePullRequest(_ pullRequests: [PullRequestEntity]) -> PullRequestEntity? {
        func newest(_ state: String) -> PullRequestEntity? {
            pullRequests.filter { $0.state == state }.reduce(nil) { best, pr in
                guard let best else { return pr }
                let diff = (ms(pr.createdAt) ?? 0) - (ms(best.createdAt) ?? 0)
                return diff > 0 || (diff == 0 && pr.number > best.number) ? pr : best
            }
        }
        return newest(DomainContract.prStateOpen) ?? newest(DomainContract.prStateMerged)
    }

    /// EXP-1307: one entry of the thread header's `…` on the Guide.
    public enum GuideMenuEntry: String, Sendable, Equatable {
        case closePr = "close-pr"
    }

    /// EXP-1307: the thread header's `…` on the Guide: Close PR while the
    /// Guide's PR (`state` = its state, nil = none) is open and the viewer is
    /// a member; empty = no trigger. EXP-1338: no GitHub entry, only the agent
    /// opens PRs.
    public static func guideMenuEntries(state: String?, member: Bool) -> [GuideMenuEntry] {
        state == DomainContract.prStateOpen && member ? [.closePr] : []
    }

    /// EXP-1318: an open PR of the thread sits in an open stack: based on
    /// another open PR, or another open PR is based on it. `teamPrs` = the
    /// team's PRs (a stack edge may cross threads). Fixture `stackCases`.
    public static func threadInStack(_ prs: [StackPr], teamPrs: [StackPr]) -> Bool {
        let open = DomainContract.prStateOpen
        let openIds = Set(teamPrs.filter { $0.state == open }.map(\.id))
        return prs.filter { $0.state == open }.contains { pr in
            (pr.basePullRequestId.map(openIds.contains) ?? false)
                || teamPrs.contains { $0.state == open && $0.basePullRequestId == pr.id }
        }
    }

    /// EXP-1309/1318: ONE quiet trailing word for a Threads row ×4 (the lock
    /// rides beside it): an open PR in a stack → `stack`; an open PR while
    /// nothing works → `readyToMerge` (working = no word); no open PR but a
    /// merged one → `merged`; else waiting on a person → `question`; else
    /// none. Fixture `wordCases`.
    public static func threadRowWord(
        prStates: [String],
        busy: Bool,
        needsYou: Bool,
        inStack: Bool
    ) -> ThreadRowWord? {
        if prStates.contains(DomainContract.prStateOpen) {
            if inStack { return .stack }
            return busy ? nil : .readyToMerge
        }
        if prStates.contains(DomainContract.prStateMerged) { return .merged }
        return needsYou ? .question : nil
    }

    /// The synced-row form: `prs` = the root's own pull requests, `teamPrs`
    /// = its team's.
    public static func threadRowWord(
        busy: Bool,
        needsYou: Bool,
        prs: [PullRequestEntity],
        teamPrs: [PullRequestEntity]
    ) -> ThreadRowWord? {
        threadRowWord(
            prStates: prs.map(\.state),
            busy: busy,
            needsYou: needsYou,
            inStack: threadInStack(prs.map(StackPr.init), teamPrs: teamPrs.map(StackPr.init))
        )
    }

    /// The rail's Active band (`ListCopy.rail`): every Needs you row, every
    /// Running row, then the Recent rows whose word still asks for a merge
    /// (`readyToMerge` or `stack`), in group order. Fixture `railCases`.
    public static func railThreads(
        _ groups: [ThreadsListGroup],
        wordOf: (ThreadsListRow) -> ThreadRowWord?
    ) -> [ThreadsListRow] {
        groups.flatMap { group -> [ThreadsListRow] in
            guard group.key == .recent else { return group.rows }
            return group.rows.filter { row in
                let word = wordOf(row)
                return word == .readyToMerge || word == .stack
            }
        }
    }

    /// EXP-1211: the quick replies above a session's composer = its NEWEST
    /// `replies` card (createdAt, then id) minus the sent items; nil = none
    /// (a newer card replaces an older one even when it is all sent).
    public static func composerReplies(sessionId: String, messages: [TreeMessage]) -> ComposerReplies? {
        var newest: TreeMessage?
        for message in messages where message.sessionId == sessionId && message.card?.kind == "replies" {
            guard let current = newest else { newest = message; continue }
            let m = ms(message.createdAt) ?? 0, n = ms(current.createdAt) ?? 0
            if m > n || (m == n && message.id > current.id) { newest = message }
        }
        guard let newest, let raw = newest.card?.items else { return nil }
        let items = raw.compactMap { item -> ComposerReply? in
            guard !item.sent, item.textIsString,
                  !item.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            else { return nil }
            return ComposerReply(id: item.id, text: item.text)
        }
        return items.isEmpty ? nil : ComposerReplies(messageId: newest.id, items: items)
    }

    // MARK: - Helpers

    /// NEW-1: a thread's issues = the UNION over its tree's rows: the
    /// sessions in tree order (the root first, then creation order), each
    /// one's rows by `sortOrder` then `issueId`, every issue once.
    public static func treeIssues(
        _ tree: [TreeSession],
        sessionIssues: [TreeSessionIssue],
        issues: [TreeIssue]
    ) -> [TreeIssue] {
        var byId: [String: TreeIssue] = [:]
        for issue in issues { byId[issue.id] = issue }
        var bySession: [String: [TreeSessionIssue]] = [:]
        for link in sessionIssues { bySession[link.sessionId, default: []].append(link) }
        var seen = Set<String>()
        var out: [TreeIssue] = []
        for session in tree {
            let rows = (bySession[session.id] ?? []).sorted { a, b in
                a.sortOrder != b.sortOrder ? a.sortOrder < b.sortOrder : a.issueId < b.issueId
            }
            for link in rows {
                guard let issue = byId[link.issueId], !seen.contains(issue.id) else { continue }
                seen.insert(issue.id)
                out.append(issue)
            }
        }
        return out
    }

    private static func eventText(
        _ card: TreeMessageCard,
        issues: [String: TreeIssue],
        pullRequests: [String: TreePullRequest],
        sessionName: (String) -> String? = { _ in nil }
    ) -> String? {
        guard let template = ThreadCopy.events[card.event ?? ""] else { return nil }
        var text = template
        if text.contains("{issue}") {
            guard let issue = card.issueId.flatMap({ issues[$0] }) else { return nil }
            text = text.replacingOccurrences(of: "{issue}", with: issue.identifier)
        }
        if text.contains("{pr}") {
            guard let pr = card.pullRequestId.flatMap({ pullRequests[$0] }) else { return nil }
            text = text.replacingOccurrences(of: "{pr}", with: "#\(pr.number)")
        }
        if text.contains("{session}") {
            // NEW-1: a bus line about a session of the tree; an unsynced one drops.
            guard let name = card.sessionId.flatMap(sessionName), !name.isEmpty else { return nil }
            text = text.replacingOccurrences(of: "{session}", with: name)
        }
        if text.contains("{name}") {
            // NEW-2: a nameless firing drops, like any unresolvable target.
            guard let name = card.eventName, !name.isEmpty else { return nil }
            text = text.replacingOccurrences(of: "{name}", with: name)
        }
        return text
    }

    /// Epoch milliseconds of a wire timestamp; nil when missing or unreadable.
    static func ms(_ value: String?) -> Double? {
        guard let value, !value.isEmpty, let date = WireTimestamps.parse(value) else { return nil }
        return ms(date)
    }

    static func ms(_ date: Date) -> Double {
        (date.timeIntervalSince1970 * 1000).rounded()
    }

    static func jsonObject(_ raw: String?) -> [String: Any]? {
        guard let raw, let data = raw.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }
}
