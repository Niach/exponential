import Foundation

/// S-3 STORIES ×4 (`packages/domain-contract/fixtures/stories.json`, TS
/// reference `src/stories.ts`): a session's own show cards (a plan, a
/// question, a result) folded into one ring per agent and per chat, for the
/// story ring + viewer. `StoriesTests` replays every fixture case.
public enum Stories {
    public enum Kind: String, Equatable, Sendable {
        case plan, question, result
    }

    public enum Footer: String, Equatable, Sendable {
        case plan, choices, reply, none
    }

    // MARK: - Fixture constants

    public enum Constants {
        public static let seenHours: Double = 24
        public static let unseenHours: Double = 168
        public static let autoAdvanceMs: Double = 6000
    }

    public enum Copy {
        public static let plan = "Plan"
        public static let question = "Question"
        public static let result = "Result"
        public static let now = "now"
        public static let approve = "Approve"
        public static let change = "Change"
        public static let reply = "Reply…"
        public static let close = "Close"
        public static let next = "Next"
        public static let previous = "Previous"

        public static func kind(_ kind: Kind) -> String {
            switch kind {
            case .plan: plan
            case .question: question
            case .result: result
            }
        }
    }

    public enum Ring {
        public static let width: Double = 3
        public static let gap: Double = 6
    }

    public enum Progress {
        public static let height: Double = 3
        public static let gap: Double = 4
    }

    public enum Viewer {
        public static let width: Double = 440
        public static let height: Double = 760
        public static let radius: Double = 16
    }

    public enum TestIds {
        public static let ring = "story-ring"
        public static let viewer = "story-viewer"
        public static let progress = "story-progress"
        public static let header = "story-header"
        public static let next = "story-next"
        public static let previous = "story-previous"
    }

    // MARK: - Input

    public struct SessionInput: Equatable, Sendable {
        public let id: String
        public let rootId: String
        /// The session's display name (session-title).
        public let name: String
        public let agentId: String?
        public let startedByAgentId: String?

        public init(id: String, rootId: String, name: String, agentId: String?, startedByAgentId: String?) {
            self.id = id
            self.rootId = rootId
            self.name = name
            self.agentId = agentId
            self.startedByAgentId = startedByAgentId
        }
    }

    public struct AgentInput: Equatable, Sendable {
        public let id: String
        public let name: String
        public let assistant: Bool

        public init(id: String, name: String, assistant: Bool) {
            self.id = id
            self.name = name
            self.assistant = assistant
        }
    }

    public struct MessageInput: Equatable, Sendable {
        public let id: String
        public let sessionId: String
        public let authorSessionId: String?
        /// Epoch ms.
        public let createdAt: Double
        public let answeredAt: Double?
        public let card: TreeMessageCard?

        public init(
            id: String, sessionId: String, authorSessionId: String?, createdAt: Double, answeredAt: Double?,
            card: TreeMessageCard?
        ) {
            self.id = id
            self.sessionId = sessionId
            self.authorSessionId = authorSessionId
            self.createdAt = createdAt
            self.answeredAt = answeredAt
            self.card = card
        }
    }

    public struct ReadInput: Equatable, Sendable {
        /// The root session of the read mark.
        public let sessionId: String
        public let readAt: Double?
        public let storySeenAt: Double?

        public init(sessionId: String, readAt: Double?, storySeenAt: Double?) {
            self.sessionId = sessionId
            self.readAt = readAt
            self.storySeenAt = storySeenAt
        }
    }

    // MARK: - Output

    public struct Picture: Equatable, Sendable {
        public let label: String
        public let attachmentId: String
        public let width: Double?
        public let height: Double?
        public let caption: String?

        public init(label: String, attachmentId: String, width: Double?, height: Double?, caption: String?) {
            self.label = label
            self.attachmentId = attachmentId
            self.width = width
            self.height = height
            self.caption = caption
        }
    }

    public struct Story: Equatable, Sendable, Identifiable {
        public let id: String
        public let sessionId: String
        public let rootId: String
        public let sessionName: String
        public let kind: Kind
        public let waiting: Bool
        public let seen: Bool
        public let createdAt: Double
        public let meta: String
        public let title: String?
        public let pictures: [Picture]
        public let surface: Bool
        public let text: String?
        public let footer: Footer
        public let autoAdvanceMs: Double?

        public init(
            id: String, sessionId: String, rootId: String, sessionName: String, kind: Kind, waiting: Bool, seen: Bool,
            createdAt: Double, meta: String, title: String?, pictures: [Picture], surface: Bool, text: String?,
            footer: Footer, autoAdvanceMs: Double?
        ) {
            self.id = id
            self.sessionId = sessionId
            self.rootId = rootId
            self.sessionName = sessionName
            self.kind = kind
            self.waiting = waiting
            self.seen = seen
            self.createdAt = createdAt
            self.meta = meta
            self.title = title
            self.pictures = pictures
            self.surface = surface
            self.text = text
            self.footer = footer
            self.autoAdvanceMs = autoAdvanceMs
        }
    }

    public struct StoryRing: Equatable, Sendable, Identifiable {
        public let key: String
        public let agentId: String?
        public let rootId: String?
        public let title: String
        public let assistant: Bool
        public var waiting: Int
        public var unseen: Bool
        public var stories: [Story]
        public var id: String { key }

        public init(
            key: String, agentId: String?, rootId: String?, title: String, assistant: Bool, waiting: Int,
            unseen: Bool, stories: [Story]
        ) {
            self.key = key
            self.agentId = agentId
            self.rootId = rootId
            self.title = title
            self.assistant = assistant
            self.waiting = waiting
            self.unseen = unseen
            self.stories = stories
        }
    }

    // MARK: - The fold

    static func resolvedPictures(_ card: TreeMessageCard) -> [Picture] {
        card.pictures.compactMap { picture in
            guard let attachmentId = picture.attachmentId else { return nil }
            return Picture(
                label: picture.label, attachmentId: attachmentId, width: picture.width, height: picture.height,
                caption: picture.caption
            )
        }
    }

    /// The card's story flag: explicit, else ask/plan or a resolved picture.
    public static func isStoryCard(_ card: TreeMessageCard?) -> Bool {
        guard let card, card.kind == "plan" || card.kind == "question" || card.kind == "surface" else { return false }
        if card.questionId != nil { return false }
        if let story = card.story { return story }
        return card.kind != "surface" || !resolvedPictures(card).isEmpty
    }

    /// `<m>m` / `<h>h` / `<d>d`, floored; under a minute = `now`.
    public static func age(_ createdAt: Double, now: Double) -> String {
        let minutes = Int((max(0, now - createdAt) / 60_000).rounded(.down))
        if minutes < 1 { return Copy.now }
        if minutes < 60 { return "\(minutes)m" }
        let hours = minutes / 60
        if hours < 24 { return "\(hours)h" }
        return "\(hours / 24)d"
    }

    private static let hour: Double = 3_600_000

    private static func byCreated(_ a: Story, _ b: Story) -> Bool {
        a.createdAt != b.createdAt ? a.createdAt < b.createdAt : a.id < b.id
    }

    private static func blank(_ value: String?) -> String? {
        guard let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines), !trimmed.isEmpty else {
            return nil
        }
        return trimmed
    }

    /// THE fold: the viewer's story rings, in display order.
    public static func rings(
        now: Double, sessions: [SessionInput], agents: [AgentInput], messages: [MessageInput], reads: [ReadInput]
    ) -> [StoryRing] {
        let sessionsById = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let agentsById = Dictionary(agents.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let marks = Dictionary(
            reads.map { ($0.sessionId, max($0.readAt ?? -.infinity, $0.storySeenAt ?? -.infinity)) },
            uniquingKeysWith: { _, last in last }
        )
        var order: [String] = []
        var byKey: [String: StoryRing] = [:]

        for message in messages {
            guard let author = message.authorSessionId, author == message.sessionId else { continue }
            guard isStoryCard(message.card), let card = message.card else { continue }
            guard let session = sessionsById[message.sessionId], let root = sessionsById[session.rootId] else {
                continue
            }
            let kind: Kind = card.kind == "plan" ? .plan : card.kind == "question" ? .question : .result
            let waiting: Bool = switch kind {
            case .plan: card.decision == nil
            case .question: message.answeredAt == nil && card.answer == nil
            case .result: false
            }
            if kind != .result, !waiting { continue }
            let seen = message.createdAt <= (marks[root.id] ?? -.infinity)
            if kind == .result {
                let keep = seen ? Constants.seenHours : Constants.unseenHours
                if now - message.createdAt >= keep * hour { continue }
            }
            let pictures = resolvedPictures(card)
            let surface = card.surface != nil
            let title: String? = switch kind {
            case .result: blank(card.title)
            case .question: blank(card.question)
            case .plan: blank(AskDock.title(card.plan ?? ""))
            }
            let story = Story(
                id: message.id,
                sessionId: session.id,
                rootId: root.id,
                sessionName: session.name,
                kind: kind,
                waiting: waiting,
                seen: seen,
                createdAt: message.createdAt,
                meta: "\(Copy.kind(kind)) · \(age(message.createdAt, now: now))",
                title: title,
                pictures: pictures,
                surface: surface,
                text: kind == .plan && pictures.isEmpty && !surface ? card.plan : nil,
                footer: kind == .plan ? .plan : kind == .question ? (card.options.isEmpty ? .reply : .choices) : .none,
                autoAdvanceMs: kind == .result ? Constants.autoAdvanceMs : nil
            )
            let agentId = root.agentId ?? root.startedByAgentId
            let agent = agentId.flatMap { agentsById[$0] }
            let key = agentId.map { "agent:\($0)" } ?? "root:\(root.id)"
            if byKey[key] == nil {
                order.append(key)
                byKey[key] = StoryRing(
                    key: key,
                    agentId: agentId,
                    rootId: agentId == nil ? root.id : nil,
                    title: agentId != nil ? (agent?.name ?? root.name) : root.name,
                    assistant: agent?.assistant ?? false,
                    waiting: 0,
                    unseen: false,
                    stories: []
                )
            }
            byKey[key]?.stories.append(story)
        }

        var list: [StoryRing] = order.compactMap { byKey[$0] }
        for index in list.indices {
            let stories = list[index].stories
            let waiting = stories.filter(\.waiting).sorted(by: byCreated)
            let unseen = stories.filter { !$0.waiting && !$0.seen }.sorted(by: byCreated)
            let seen = stories.filter { !$0.waiting && $0.seen }.sorted(by: byCreated)
            list[index].stories = waiting + unseen + seen
            list[index].waiting = waiting.count
            list[index].unseen = list[index].stories.contains { $0.waiting || !$0.seen }
        }
        func rank(_ ring: StoryRing) -> Int { ring.assistant ? 0 : ring.waiting > 0 ? 1 : ring.unseen ? 2 : 3 }
        func newest(_ ring: StoryRing) -> Double { ring.stories.map(\.createdAt).max() ?? 0 }
        return list.sorted { a, b in
            if rank(a) != rank(b) { return rank(a) < rank(b) }
            if newest(a) != newest(b) { return newest(a) > newest(b) }
            return a.key < b.key
        }
    }

    /// The chats header row (chats.json BUBBLES + STORY RING): each agent
    /// bubble wears its agent's ring (`byAgent`, keyed by agentId, first
    /// wins); `rest` = the rings no bubble wears (the root rings, an agent
    /// ring with no bubble), in ring order, painted after the bubbles in the
    /// same row.
    public static func bubbleRings(
        _ bubbleAgentIds: [String], _ rings: [StoryRing]
    ) -> (byAgent: [String: StoryRing], rest: [StoryRing]) {
        let ids = Set(bubbleAgentIds)
        var byAgent: [String: StoryRing] = [:]
        var rest: [StoryRing] = []
        for ring in rings {
            if let agentId = ring.agentId, ids.contains(agentId), byAgent[agentId] == nil {
                byAgent[agentId] = ring
            } else {
                rest.append(ring)
            }
        }
        return (byAgent, rest)
    }

    /// Epoch ms of a synced wire timestamp (nil = absent or unparseable).
    public static func ms(_ wire: String?) -> Double? {
        wire.flatMap(WireTimestamps.parse).map { $0.timeIntervalSince1970 * 1000 }
    }
}
