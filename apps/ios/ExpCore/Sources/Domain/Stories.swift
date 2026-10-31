import Foundation

/// S-3 STORIES ×4 (`packages/domain-contract/fixtures/stories.json`, TS
/// reference `src/stories.ts`): a session's own show cards (a plan, a
/// question, a result) folded into one ring per agent and per chat, for the
/// story ring + viewer. `StoriesTests` replays every fixture case.
public enum Stories {
    public enum Kind: String, Equatable, Sendable {
        case plan, question, result
    }

    /// `open` = a plan: THE plan page (plan-page.json), never a decision in a story.
    public enum Footer: String, Equatable, Sendable {
        case open, choices, reply, none
    }

    // MARK: - Fixture constants

    public enum Constants {
        /// #EXP-1461: a ring keeps a story younger than this, waiting or not.
        public static let windowHours: Double = 24
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
        /// Retired (#EXP-1493: one segment per story, `gap` apart); the
        /// fixture keeps it for old mirrors.
        public static let storyGap: Double = 10
    }

    /// #EXP-1493 THE kind badge ×4 (eUI Badge, the pill): the concept icon +
    /// the kind word, text in the tint, the tint at `fill` behind, at
    /// `stroke` as its border.
    public enum Badge {
        public static func icon(_ kind: Kind) -> String {
            switch kind {
            case .plan: "coding-plan"
            case .question: "ui-help"
            case .result: "ui-check"
            }
        }

        /// The ask-dock tints: plan primary, question yellow, result emerald.
        public static func tint(_ kind: Kind) -> String {
            switch kind {
            case .plan: "primary"
            case .question: "yellow"
            case .result: "emerald"
            }
        }

        public static let fill: Double = 0.14
        public static let stroke: Double = 0.3
        public static let height: Double = 20
    }

    public enum Viewer {
        public static let width: Double = 440
        public static let height: Double = 760
        public static let radius: Double = 16
        /// Phones: the floating top band's scrim (and its mirrored footer
        /// scrim) is at least this tall, fading clear over the last `scrimFade`.
        public static let scrimHeight: Double = 180
        public static let scrimFade: Double = 48
        /// #EXP-1463: the wide modal's room on each side (x) and top/bottom (y).
        public static let insetX: Double = 96
        public static let insetY: Double = 48
        public static let minWidth: Double = 360
    }

    /// #EXP-1461 THE STORY ROW's geometry (`row`).
    public enum Row {
        public static let itemWidth: Double = 72
        public static let gap: Double = 12
        public static let labelGap: Double = 6
        public static let padBottom: Double = 4
    }

    public enum TestIds {
        public static let ring = "story-ring"
        public static let viewer = "story-viewer"
        public static let progress = "story-progress"
        public static let header = "story-header"
        public static let next = "story-next"
        public static let previous = "story-previous"
    }

    /// The session screen's Stories tab (`tab`).
    public enum Tab {
        public enum Copy {
            public static let label = "Stories"
            public static let empty = "No stories yet"
            public static let approved = "Approved"
            public static let changed = "Changes asked"
            public static let answered = "Answered"
            public static let superseded = "Replaced"
            public static let changes = "Changes"
            public static let stack = "Stack"
        }

        public enum TestIds {
            public static let tab = "thread-stories-tab"
            public static let list = "thread-stories"
            public static let tile = "story-tile"
            public static let changes = "thread-stories-changes"
            public static let stack = "thread-stories-stack"
        }

        /// A tile's geometry; iOS = the `phone` form factor.
        public enum Tile {
            public static let thumbHeight: Double = 240
            public static let thumbMaxWidth: Double = 320
            public static let gap: Double = 8
            public static let textGap: Double = 4
            public static let radius: Double = 10
            public static let listGap: Double = 32
            public static let leadGap: Double = 24

            /// A thumbnail's width: by aspect at `thumbHeight` (unknown = 4:3),
            /// capped at `thumbMaxWidth`.
            public static func thumbWidth(width: Double?, height: Double?) -> Double {
                guard let width, let height, width > 0, height > 0 else {
                    return min(thumbMaxWidth, (thumbHeight * 4 / 3).rounded())
                }
                return min(thumbMaxWidth, (thumbHeight * width / height).rounded())
            }
        }

        /// The styleguide specimen draws `tabCases[specimen.case]`.
        public static let specimenCase = 1
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
        /// The card's `description` (a result's one line under its title).
        public let description: String?
        public let pictures: [Picture]
        public let surface: Bool
        /// A plan's counts (plan-page `counts`), else nil.
        public let counts: PlanPage.Counts?
        public let footer: Footer
        public let autoAdvanceMs: Double?

        public init(
            id: String, sessionId: String, rootId: String, sessionName: String, kind: Kind, waiting: Bool, seen: Bool,
            createdAt: Double, meta: String, title: String?, description: String? = nil, pictures: [Picture],
            surface: Bool, counts: PlanPage.Counts?, footer: Footer, autoAdvanceMs: Double?
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
            self.description = description
            self.pictures = pictures
            self.surface = surface
            self.counts = counts
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

    /// #EXP-1493: newest first (createdAt desc, then id desc).
    private static func byNewest(_ a: Story, _ b: Story) -> Bool {
        a.createdAt != b.createdAt ? a.createdAt > b.createdAt : a.id > b.id
    }

    private static func blank(_ value: String?) -> String? {
        guard let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines), !trimmed.isEmpty else {
            return nil
        }
        return trimmed
    }

    /// How a plan/question was settled (nil while waiting, and for results).
    enum Resolution: Equatable {
        case approved, changed, answered, superseded
    }

    /// One message as a story (nil = not a story of a known root).
    static func storyOf(
        _ message: MessageInput, sessions: [String: SessionInput], marks: [String: Double], now: Double
    ) -> (story: Story, root: SessionInput, resolution: Resolution?)? {
        guard let author = message.authorSessionId, author == message.sessionId else { return nil }
        guard isStoryCard(message.card), let card = message.card else { return nil }
        guard let session = sessions[message.sessionId], let root = sessions[session.rootId] else { return nil }
        let kind: Kind = card.kind == "plan" ? .plan : card.kind == "question" ? .question : .result
        let waiting: Bool = switch kind {
        case .plan: card.decision == nil
        case .question: message.answeredAt == nil && card.answer == nil
        case .result: false
        }
        let resolution: Resolution? = kind == .result || waiting
            ? nil
            : kind == .question ? .answered
            : card.decision == "approved" ? .approved
            : card.decision == "superseded" ? .superseded
            : .changed
        let pictures = resolvedPictures(card)
        let surface = card.surface != nil
        let title: String? = switch kind {
        case .result: blank(card.title)
        case .question: blank(card.question)
        case .plan: blank(AskDock.title(card.plan ?? ""))
        }
        let ageText = age(message.createdAt, now: now)
        let meta: String = switch resolution {
        case .approved: "\(Copy.kind(kind)) · \(Tab.Copy.approved) · \(ageText)"
        case .changed: "\(Copy.kind(kind)) · \(Tab.Copy.changed) · \(ageText)"
        case .answered: "\(Copy.kind(kind)) · \(Tab.Copy.answered) · \(ageText)"
        case .superseded, nil: "\(Copy.kind(kind)) · \(ageText)"
        }
        let footer: Footer = kind == .plan
            ? .open
            : resolution != nil ? .none
            : kind == .question ? (card.options.isEmpty ? .reply : .choices) : .none
        let story = Story(
            id: message.id,
            sessionId: session.id,
            rootId: root.id,
            sessionName: session.name,
            kind: kind,
            waiting: waiting,
            // A settled plan/question was acted on: seen.
            seen: resolution != nil || message.createdAt <= (marks[root.id] ?? -.infinity),
            createdAt: message.createdAt,
            meta: meta,
            title: title,
            description: blank(card.description),
            pictures: pictures,
            surface: surface,
            counts: kind == .plan ? PlanPage.counts(card) : nil,
            footer: footer,
            autoAdvanceMs: kind == .result ? Constants.autoAdvanceMs : nil
        )
        return (story, root, resolution)
    }

    /// The ring a root's stories ride (an agent's, else the chat's), empty.
    static func emptyRing(_ root: SessionInput, agents: [String: AgentInput]) -> StoryRing {
        let agentId = root.agentId ?? root.startedByAgentId
        let agent = agentId.flatMap { agents[$0] }
        return StoryRing(
            key: agentId.map { "agent:\($0)" } ?? "root:\(root.id)",
            agentId: agentId,
            rootId: agentId == nil ? root.id : nil,
            title: agentId != nil ? (agent?.name ?? root.name) : root.name,
            assistant: agent?.assistant ?? false,
            waiting: 0,
            unseen: false,
            stories: []
        )
    }

    private static func marksOf(_ reads: [ReadInput]) -> [String: Double] {
        Dictionary(
            reads.map { ($0.sessionId, max($0.readAt ?? -.infinity, $0.storySeenAt ?? -.infinity)) },
            uniquingKeysWith: { _, last in last }
        )
    }

    /// THE fold: the viewer's story rings, in display order.
    public static func rings(
        now: Double, sessions: [SessionInput], agents: [AgentInput], messages: [MessageInput], reads: [ReadInput]
    ) -> [StoryRing] {
        let sessionsById = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let agentsById = Dictionary(agents.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let marks = marksOf(reads)
        var order: [String] = []
        var byKey: [String: StoryRing] = [:]

        for message in messages {
            // A settled plan/question leaves the ring (the Stories tab keeps it).
            guard let built = storyOf(message, sessions: sessionsById, marks: marks, now: now),
                  built.resolution == nil else { continue }
            let story = built.story
            // #EXP-1461: a ring holds the last `windowHours` only, waiting or not.
            if now - story.createdAt >= Constants.windowHours * hour { continue }
            let ring = emptyRing(built.root, agents: agentsById)
            if byKey[ring.key] == nil {
                order.append(ring.key)
                byKey[ring.key] = ring
            }
            byKey[ring.key]?.stories.append(story)
        }

        var list: [StoryRing] = order.compactMap { byKey[$0] }
        for index in list.indices {
            // #EXP-1493: a session's newer result replaces its older ones in a ring.
            var newestResult: [String: Story] = [:]
            for story in list[index].stories where story.kind == .result {
                if let kept = newestResult[story.sessionId], !byNewest(story, kept) { continue }
                newestResult[story.sessionId] = story
            }
            let stories = list[index].stories.filter { $0.kind != .result || newestResult[$0.sessionId]?.id == $0.id }
            let waiting = stories.filter(\.waiting).sorted(by: byNewest)
            let rest = stories.filter { !$0.waiting }.sorted(by: byNewest)
            list[index].stories = waiting + rest
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

    /// THE Stories tab of a session screen (stories.json `tab`): every story
    /// the root's tree ever posted, never expiring; settled plans/questions
    /// stay (footer none, a plan keeps `open`; `meta` names the outcome), a
    /// superseded plan drops. Waiting first, then the rest, each newest first. ONE ring
    /// (the root's, as `rings` keys it) so the viewer opens on it; nil = the
    /// empty state. `unseen` = the tab's dot.
    public static func storyTab(
        now: Double, sessions: [SessionInput], agents: [AgentInput], messages: [MessageInput], reads: [ReadInput],
        rootId: String
    ) -> StoryRing? {
        let sessionsById = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let agentsById = Dictionary(agents.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        guard let root = sessionsById[rootId], root.rootId == root.id else { return nil }
        let marks = marksOf(reads)
        var list: [Story] = []
        for message in messages {
            guard let built = storyOf(message, sessions: sessionsById, marks: marks, now: now),
                  built.root.id == root.id, built.resolution != .superseded else { continue }
            list.append(built.story)
        }
        if list.isEmpty { return nil }
        let waiting = list.filter(\.waiting).sorted(by: byNewest)
        let rest = list.filter { !$0.waiting }.sorted(by: byNewest)
        var ring = emptyRing(root, agents: agentsById)
        ring.waiting = waiting.count
        ring.unseen = list.contains { $0.waiting || !$0.seen }
        ring.stories = waiting + rest
        return ring
    }

    // MARK: - Frames (STORY VIEWER)

    public enum FrameKind: String, Equatable, Sendable {
        case plan, gallery, surface, title
    }

    /// THE viewer's ONE step per story (#EXP-1493): a plan's cover, a
    /// gallery (the pictures swipe inside the frame), a surface, or a bare title.
    public struct Frame: Equatable, Sendable {
        /// The story's index in the ring and its id.
        public let story: Int
        public let storyId: String
        public let kind: FrameKind
        /// The story's resolved pictures (a plan's cover = the first).
        public let pictures: Int
        public let title: String?
        public let description: String?
        public let footer: Footer
        /// Per picture on a gallery (the frame lasts pictures × it), else the frame's.
        public let autoAdvanceMs: Double?

        public init(
            story: Int, storyId: String, kind: FrameKind, pictures: Int, title: String?, description: String?,
            footer: Footer, autoAdvanceMs: Double?
        ) {
            self.story = story
            self.storyId = storyId
            self.kind = kind
            self.pictures = pictures
            self.title = title
            self.description = description
            self.footer = footer
            self.autoAdvanceMs = autoAdvanceMs
        }
    }

    /// A story's frame: a plan = its cover; else pictures win (a gallery: the
    /// surface stays in the chat), else the surface, else the bare title.
    static func frameOf(_ story: Story, at: Int) -> Frame {
        let kind: FrameKind = story.kind == .plan ? .plan
            : !story.pictures.isEmpty ? .gallery
            : story.surface ? .surface : .title
        return Frame(
            story: at, storyId: story.id, kind: kind, pictures: story.pictures.count, title: story.title,
            description: story.description, footer: story.footer, autoAdvanceMs: story.autoAdvanceMs
        )
    }

    /// THE viewer's steps over a ring (`frameCases`): ONE frame per story, in
    /// ring order.
    public static func frames(_ stories: [Story]) -> [Frame] {
        stories.enumerated().map { at, story in frameOf(story, at: at) }
    }

    /// The frame index of a story; -1 = none. A thumbnail opens its story's
    /// frame with the gallery at that picture (the host's state).
    public static func frameIndex(_ frames: [Frame], storyId: String) -> Int {
        frames.firstIndex { $0.storyId == storyId } ?? -1
    }

    /// THE STORY ROW of the chats header (#EXP-1461, `rowCases`): ONLY
    /// rings. `bubbles` = the agent bubbles (`bubbleAgentIds` in listAgents
    /// order) whose agent has a ring, each wearing it (`byAgent`, first
    /// wins); `rest` = the rings no bubble wears (the root rings, an agent
    /// ring with no bubble), in ring order. A bubble without a ring is not
    /// drawn; both empty = no row at all.
    public static func storyRow(
        _ bubbleAgentIds: [String], _ rings: [StoryRing]
    ) -> (bubbles: [String], byAgent: [String: StoryRing], rest: [StoryRing]) {
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
        var seen = Set<String>()
        let bubbles = bubbleAgentIds.filter { seen.insert($0).inserted && byAgent[$0] != nil }
        return (bubbles, byAgent, rest)
    }

    /// THE wide viewer's modal size (#EXP-1463, `sizeCases`): a picture with
    /// a known size = fitted (contain) into the viewport less the inset, at
    /// least `minWidth` wide; anything else = `Viewer.width` × `height`, each
    /// capped by the same box. Whole px. Phones never call it (full screen).
    public static func viewerSize(
        picture: (width: Double?, height: Double?)?, viewport: (width: Double, height: Double)
    ) -> (width: Double, height: Double) {
        let boxWidth = max(0, viewport.width - 2 * Viewer.insetX)
        let boxHeight = max(0, viewport.height - 2 * Viewer.insetY)
        guard let picture, let pw = picture.width, let ph = picture.height, pw > 0, ph > 0 else {
            return (min(Viewer.width, boxWidth), min(Viewer.height, boxHeight))
        }
        let scale = min(boxWidth / pw, boxHeight / ph)
        let width = (pw * scale).rounded()
        return (min(boxWidth, max(Viewer.minWidth, width)), (ph * scale).rounded())
    }

    /// Epoch ms of a synced wire timestamp (nil = absent or unparseable).
    public static func ms(_ wire: String?) -> Double? {
        wire.flatMap(WireTimestamps.parse).map { $0.timeIntervalSince1970 * 1000 }
    }
}
