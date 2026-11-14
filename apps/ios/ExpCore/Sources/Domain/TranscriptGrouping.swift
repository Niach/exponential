import Foundation

// #EXP-1736: ONE transcript ×4, every chat a group chat. Who sent each row,
// which rows share an avatar, the gaps between them.
// fixtures/transcript-grouping.json holds the rules + cases; mirrors: TS
// `src/transcript-grouping.ts`, desktop `domain::transcript_grouping`,
// Android `domain/TranscriptGrouping.kt`.
public enum TranscriptGrouping {
    /// Fixture `layout` (points).
    public enum Layout {
        /// THE Avatar's default size beside a group.
        public static let avatar: CGFloat = 32
        /// The avatar column's gap to the bubble column.
        public static let avatarGap: CGFloat = 8
        /// The space above a row inside a group.
        public static let gapInGroup: CGFloat = 3
        /// The space above a row that starts a group.
        public static let gapBetween: CGFloat = 12
        /// A bubble's share of its column on a phone / a wide screen.
        public static let maxWidthPhone: CGFloat = 0.78
        /// Anyone but the viewer's bubble on a phone.
        public static let maxWidthPhoneIn: CGFloat = 1
        public static let maxWidthWide: CGFloat = 0.62
        /// #FEED-239: the viewer's OWN bubble on every width (the column
        /// left of the own avatar; no left inset).
        public static let maxWidthOwn: CGFloat = 1
        public static let radius: CGFloat = 16
        public static let nameSize: CGFloat = 12
    }

    /// Fixture `copy`.
    public enum Copy {
        public static let formerMember = "Former member"
        public static let linkChevron = "›"
        /// #FEED-238 THE FOLD's text button.
        public static let showMore = "Show more"
        public static let showLess = "Show less"
        /// #EXP-1772 `copy.markers`: a system input's marker label by key.
        public static let markers: [String: String] = [
            "schedule": "Schedule",
            "event": "Event",
            "pr_opened": "Pull request opened",
            "pr_merged": "Pull request merged",
            "pr_merge": "Merge",
            "pr_checks": "CI",
            "session": "Session update",
        ]
    }

    public enum ItemKind: String, Equatable, Sendable {
        /// A card-less message row, a child's report, a pending input.
        case message
        /// A tool, thought or subagent line of the agent's work.
        case trace
        /// Every card: full width, never grouped.
        case full
    }

    public struct Item: Equatable, Sendable {
        public let id: String
        public let kind: ItemKind
        /// `message` only: `user` | `agent` | `system`.
        public let authorKind: String?
        public let authorId: String?
        public let authorSessionId: String?

        public init(
            id: String,
            kind: ItemKind,
            authorKind: String? = nil,
            authorId: String? = nil,
            authorSessionId: String? = nil
        ) {
            self.id = id
            self.kind = kind
            self.authorKind = authorKind
            self.authorId = authorId
            self.authorSessionId = authorSessionId
        }
    }

    public struct Chat: Equatable, Sendable {
        /// The session whose conversation this is.
        public let sessionId: String
        public let title: String
        public let agentId: String?
        /// The session that started it (session-chat.json `origin`, from the
        /// synced rows), else nil.
        public let originSessionId: String?
        /// The items begin at the conversation's first message (fixture
        /// `fromStart`, absent = true).
        public let fromStart: Bool

        public init(
            sessionId: String,
            title: String,
            agentId: String?,
            originSessionId: String?,
            fromStart: Bool = true
        ) {
            self.sessionId = sessionId
            self.title = title
            self.agentId = agentId
            self.originSessionId = originSessionId
            self.fromStart = fromStart
        }
    }

    /// A member or an agent: its id and display name.
    public struct Named: Equatable, Sendable {
        public let id: String
        public let name: String

        public init(id: String, name: String) {
            self.id = id
            self.name = name
        }
    }

    public struct KnownSession: Equatable, Sendable {
        public let id: String
        public let title: String
        public let agentId: String?

        public init(id: String, title: String, agentId: String?) {
            self.id = id
            self.title = title
            self.agentId = agentId
        }
    }

    public struct Input: Equatable, Sendable {
        public let viewerId: String?
        public let chat: Chat
        public let members: [Named]
        public let agents: [Named]
        public let sessions: [KnownSession]
        public let items: [Item]

        public init(
            viewerId: String?,
            chat: Chat,
            members: [Named],
            agents: [Named],
            sessions: [KnownSession],
            items: [Item]
        ) {
            self.viewerId = viewerId
            self.chat = chat
            self.members = members
            self.agents = agents
            self.sessions = sessions
            self.items = items
        }
    }

    public enum SenderKind: String, Equatable, Sendable {
        case member
        case agent
        case session
    }

    public struct Sender: Equatable, Hashable, Sendable {
        public let kind: SenderKind
        public let id: String
        public let name: String
        /// A cross-session sender: the session a press opens.
        public let link: String?

        public init(kind: SenderKind, id: String, name: String, link: String?) {
            self.kind = kind
            self.id = id
            self.name = name
            self.link = link
        }

        /// The bubble's name line: a cross-session sender's ends in ` ›`.
        public var displayName: String {
            link == nil ? name : "\(name) \(Copy.linkChevron)"
        }
    }

    public enum Side: String, Equatable, Sendable {
        case own
        case other
        case full
    }

    public struct RowLayout: Equatable, Sendable {
        public let id: String
        public let side: Side
        public let sender: Sender?
        /// The sender's avatar beside this row (a group's first; #FEED-239:
        /// own groups too, the viewer's avatar on the RIGHT).
        public let avatar: Bool
        /// The sender's name on this row (a group's first message, not own).
        public let name: Bool
        /// The space above the row.
        public let gap: CGFloat

        public init(id: String, side: Side, sender: Sender?, avatar: Bool, name: Bool, gap: CGFloat) {
            self.id = id
            self.side = side
            self.sender = sender
            self.avatar = avatar
            self.name = name
            self.gap = gap
        }
    }

    public static func transcriptLayout(_ input: Input) -> [RowLayout] {
        let chat = input.chat
        let sessions = Dictionary(input.sessions.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let members = Dictionary(input.members.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
        let agents = Dictionary(input.agents.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
        func fromSession(_ id: String?) -> Sender? {
            guard let id, !id.isEmpty, id != chat.sessionId, let session = sessions[id] else { return nil }
            if let agentId = session.agentId, !agentId.isEmpty {
                return Sender(kind: .agent, id: agentId, name: session.title, link: session.id)
            }
            return Sender(kind: .session, id: session.id, name: session.title, link: session.id)
        }
        let ownAgent: Sender = {
            if let agentId = chat.agentId, !agentId.isEmpty {
                return Sender(kind: .agent, id: agentId, name: agents[agentId] ?? chat.title, link: nil)
            }
            return Sender(kind: .session, id: chat.sessionId, name: chat.title, link: nil)
        }()
        var originPending = !(chat.originSessionId ?? "").isEmpty && chat.fromStart

        var out: [RowLayout] = []
        var prevKey: String?
        var groupNamed = false
        for item in input.items {
            let side: Side
            var sender: Sender?
            if item.kind == .full || (item.kind == .message && item.authorKind == "system") {
                side = .full
            } else if item.kind == .trace {
                side = .other
                sender = ownAgent
            } else {
                let viewer = input.viewerId ?? ""
                let own = item.authorKind == "user" && !viewer.isEmpty && item.authorId == viewer
                let cross = fromSession(item.authorSessionId)
                // The origin: the conversation's first INPUT (a person's row
                // or a known session's), unless it names its own sender; the
                // agent's own output never takes it.
                let isInput = item.authorKind == "user" || cross != nil
                let origin = originPending && isInput && cross == nil ? fromSession(chat.originSessionId) : nil
                if isInput { originPending = false }
                if let cross {
                    side = .other
                    sender = cross
                } else if let origin {
                    side = .other
                    sender = origin
                } else if own {
                    side = .own
                } else {
                    side = .other
                    if item.authorKind == "user" {
                        let id = item.authorId ?? ""
                        sender = Sender(kind: .member, id: id, name: members[id] ?? Copy.formerMember, link: nil)
                    } else {
                        sender = ownAgent
                    }
                }
            }
            let key: String?
            switch side {
            case .full: key = nil
            case .own: key = "own"
            case .other: key = sender.map { "\($0.kind.rawValue):\($0.id):\($0.link ?? "")" }
            }
            let sameGroup = key != nil && key == prevKey
            if !sameGroup { groupNamed = false }
            let name = side == .other && item.kind == .message && !groupNamed
            if name { groupNamed = true }
            out.append(RowLayout(
                id: item.id,
                side: side,
                sender: sender,
                // #FEED-239: the viewer's own group wears THEIR avatar on the right.
                avatar: side != .full && !sameGroup,
                name: name,
                gap: out.isEmpty ? 0 : sameGroup ? Layout.gapInGroup : Layout.gapBetween
            ))
            prevKey = key
        }
        return out
    }

    /// #FEED-238 THE FOLD ×4 (fixture `fold`).
    public enum Fold {
        public static let lines = 6
        public static let chars = 600
    }

    /// THE FOLD (`foldCases`): a message bubble longer than `Fold.chars`
    /// characters (UTF-16, as the TS reference counts) or `Fold.lines` lines
    /// folds behind Show more.
    public static func messageFolds(_ text: String) -> Bool {
        text.utf16.count > Fold.chars || text.components(separatedBy: "\n").count > Fold.lines
    }

    /// A session input as the device words it (`coding::pickup::input_text`).
    public struct CrossSessionInput: Equatable, Sendable {
        public enum Kind: String, Equatable, Sendable {
            case report
            case message
        }

        public let kind: Kind
        public let name: String
        public let sessionId: String
        public let body: String

        public init(kind: Kind, name: String, sessionId: String, body: String) {
            self.kind = kind
            self.name = name
            self.sessionId = sessionId
            self.body = body
        }
    }

    private static let inputPattern = try? NSRegularExpression(
        pattern: #"^(Report|Message) from (.+?) \(session ([^,)\s]+)(, ended)?\): ([\s\S]*)$"#
    )

    /// `Report from <name> (session <id>, ended): <summary>` /
    /// `Message from <name> (session <id>): <body>`; anything else = nil.
    public static func crossSessionInput(_ text: String) -> CrossSessionInput? {
        guard let pattern = inputPattern else { return nil }
        let ns = text as NSString
        guard let match = pattern.firstMatch(in: text, range: NSRange(location: 0, length: ns.length)),
              match.range.location == 0, match.range.length == ns.length
        else { return nil }
        func group(_ index: Int) -> String? {
            let range = match.range(at: index)
            return range.location == NSNotFound ? nil : ns.substring(with: range)
        }
        guard let lead = group(1), let name = group(2), let sessionId = group(3) else { return nil }
        return CrossSessionInput(
            kind: lead == "Report" ? .report : .message,
            name: name,
            sessionId: sessionId,
            body: group(5) ?? ""
        )
    }

    // #EXP-1772: only real messages are bubbles. A system input (a schedule
    // or event firing, a PR or session status line) is THE inline marker,
    // drawn only above the agent's reply to it.

    /// A feed input: a marker (`key`, `label`) or another session's relay.
    public enum FeedInput: Equatable, Sendable {
        case marker(key: String, label: String)
        case relay(name: String, sessionId: String, body: String, report: Bool)
    }

    private static let tagPattern = try? NSRegularExpression(pattern: #"^\[([a-z_]+)\] "#)
    private static let schedulePattern = try? NSRegularExpression(pattern: #"^Schedule `[^`\n]+`[^\n]* fired"#)
    private static let eventPattern = try? NSRegularExpression(pattern: #"^Event `[^`\n]+` \(from "#)

    private static func marker(_ key: String) -> FeedInput {
        .marker(key: key, label: Copy.markers[key] ?? key)
    }

    /// The device's tagged system line = a marker, another session's input =
    /// a relay, else nil (a person's prompt).
    public static func feedInput(_ text: String) -> FeedInput? {
        let ns = text as NSString
        let all = NSRange(location: 0, length: ns.length)
        if let match = tagPattern?.firstMatch(in: text, range: all) {
            let key = ns.substring(with: match.range(at: 1))
            if Copy.markers[key] != nil { return marker(key) }
        }
        if schedulePattern?.firstMatch(in: text, range: all) != nil { return marker("schedule") }
        if eventPattern?.firstMatch(in: text, range: all) != nil { return marker("event") }
        guard let cross = crossSessionInput(text) else { return nil }
        return .relay(name: cross.name, sessionId: cross.sessionId, body: cross.body, report: cross.kind == .report)
    }

    public enum FeedRowKind: String, Equatable, Sendable {
        case marker
        case input
        case reply
        case other
    }

    /// Per row: is it drawn? A marker only when a reply follows it before
    /// the next input or marker; every other row always.
    public static func markersShown(_ kinds: [FeedRowKind]) -> [Bool] {
        var shown = kinds.map { _ in true }
        var pending: Int?
        for (index, kind) in kinds.enumerated() {
            switch kind {
            case .marker, .input:
                if let pending { shown[pending] = false }
                pending = kind == .marker ? index : nil
            case .reply:
                pending = nil
            case .other:
                break
            }
        }
        if let pending { shown[pending] = false }
        return shown
    }
}
