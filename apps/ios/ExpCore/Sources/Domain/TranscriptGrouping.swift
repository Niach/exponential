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
        /// #EXP-1873: the quiet check-in line.
        public static let checkIn = "Checked in, nothing to do"
        public static let checkIns = "Checked in {count} times, nothing to do"
        public static let checkInTimeSeparator = " · "
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
        /// #EXP-1876: THE bubble tail (a group's first message): start for
        /// others, end for own.
        public let tail: Bool
        /// The space above the row.
        public let gap: CGFloat

        public init(id: String, side: Side, sender: Sender?, avatar: Bool, name: Bool, tail: Bool, gap: CGFloat) {
            self.id = id
            self.side = side
            self.sender = sender
            self.avatar = avatar
            self.name = name
            self.tail = tail
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
        var groupTailed = false
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
            if !sameGroup {
                groupNamed = false
                groupTailed = false
            }
            let name = side == .other && item.kind == .message && !groupNamed
            if name { groupNamed = true }
            let tail = side != .full && item.kind == .message && !groupTailed
            if tail { groupTailed = true }
            out.append(RowLayout(
                id: item.id,
                side: side,
                sender: sender,
                // #FEED-239: the viewer's own group wears THEIR avatar on the right.
                avatar: side != .full && !sameGroup,
                name: name,
                tail: tail,
                gap: out.isEmpty ? 0 : sameGroup ? Layout.gapInGroup : Layout.gapBetween
            ))
            prevKey = key
        }
        return out
    }

    /// #EXP-1864 fixture `notices`: an Exponential tool row drawn as THE
    /// inline marker (its own line, never a trace beside the avatar).
    public static let notices: [String: String] = [
        "session_self_quiet": "Ended turn quietly",
    ]

    private static let noticePrefix = "exponential_"

    /// `noticeCases`: the notice label for a tool name ending in the contract
    /// prefix + a `notices` row (whatever namespace sits before it), else nil.
    public static func traceNotice(_ toolName: String?) -> String? {
        let name = (toolName ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        for (row, label) in notices where name.count > row.count && name.hasSuffix(row) {
            if name.dropLast(row.count).hasSuffix(noticePrefix) { return label }
        }
        return nil
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

    /// #EXP-1873: a feed row as `feedRows` reads it: `schedule` = a schedule
    /// firing (a check-in), `marker` = every other marker, `quiet` = the quiet
    /// notice, `blank` = an agent turn with blank text.
    public enum FeedRowRole: String, Equatable, Sendable {
        case schedule
        case marker
        case input
        case reply
        case quiet
        case blank
        case other
    }

    /// Per row: hidden, drawn as itself, or THE check-in line covering n
    /// quiet check-ins.
    public enum FeedRowShown: Equatable, Sendable {
        case hidden
        case row
        case checkIn(Int)
    }

    /// #EXP-1873 (`checkInCases`): a quiet check-in (a schedule firing whose
    /// turn only called quiet, nothing else) is one line;
    /// consecutive ones collapse onto the last.
    public static func feedRows(_ kinds: [FeedRowRole]) -> [FeedRowShown] {
        let base = markersShown(kinds.map { kind -> FeedRowKind in
            switch kind {
            case .schedule, .marker: return .marker
            case .input: return .input
            case .reply: return .reply
            case .quiet, .blank, .other: return .other
            }
        })
        var shown: [FeedRowShown] = base.enumerated().map { index, drawn in
            drawn && kinds[index] != .blank ? .row : .hidden
        }
        for (start, kind) in kinds.enumerated() where kind == .schedule {
            var end = start + 1
            while end < kinds.count, ![.schedule, .marker, .input].contains(kinds[end]) { end += 1 }
            var quiet = false
            var answered = false
            for row in kinds[(start + 1)..<end] {
                if row == .other || row == .reply { answered = true; break }
                if row == .quiet { quiet = true }
            }
            guard quiet, !answered else { continue }
            shown[start] = .checkIn(1)
            for index in (start + 1)..<end { shown[index] = .hidden }
        }
        var line: Int?
        for index in shown.indices {
            switch shown[index] {
            case .checkIn(let count):
                if let previous = line, case .checkIn(let before) = shown[previous] {
                    shown[index] = .checkIn(before + count)
                    shown[previous] = .hidden
                }
                line = index
            case .row:
                line = nil
            case .hidden:
                break
            }
        }
        return shown
    }

    /// #EXP-1873 (`checkInLabelCases`): the check-in line's text.
    public static func checkInLabel(count: Int, time: String?) -> String {
        let label = count == 1 ? Copy.checkIn : Copy.checkIns.replacingOccurrences(of: "{count}", with: String(count))
        guard let time, !time.isEmpty else { return label }
        return "\(label)\(Copy.checkInTimeSeparator)\(time)"
    }

    /// #FEED-256 / #FEED-254 fixture `blank.codePoints`: whitespace plus the
    /// invisible format characters (hex Unicode scalars).
    public static let blankCodePoints: Set<UInt32> = [
        0x0009, 0x000A, 0x000B, 0x000C, 0x000D, 0x0020, 0x0085, 0x00A0, 0x00AD,
        0x1680, 0x180E, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005, 0x2006,
        0x2007, 0x2008, 0x2009, 0x200A, 0x200B, 0x200C, 0x200D, 0x200E, 0x200F,
        0x2028, 0x2029, 0x202F, 0x205F, 0x2060, 0x3000, 0xFEFF,
    ]

    /// #FEED-256 / #FEED-254 (`blankCases`): a text whose every scalar is in
    /// `blankCodePoints` is no message: no client draws it. Never
    /// `trimmingCharacters`: it disagrees on U+200B, U+FEFF and U+00A0.
    public static func isBlankText(_ text: String?) -> Bool {
        (text ?? "").unicodeScalars.allSatisfy { blankCodePoints.contains($0.value) }
    }

    /// #EXP-1873 fixture `checkInSpecimen`: an agent chat's feed the
    /// chat-transcript specimen draws through `feedRows`.
    public struct CheckInSpecimenRow: Equatable, Sendable {
        public let kind: FeedRowRole
        public let time: String?
        public let text: String?
    }

    /// #EXP-1873 fixture `checkInSpecimenAgent`: the agent whose real
    /// bubble (avatar, name, tail) answers in the specimen.
    public struct CheckInSpecimenAgent: Equatable, Sendable {
        public let name: String
        public let icon: String?
        public let color: String?
    }

    public static let checkInSpecimenAgent = CheckInSpecimenAgent(name: "Assistant", icon: "bot", color: "#6366f1")

    public static let checkInSpecimen: [CheckInSpecimenRow] = [
        CheckInSpecimenRow(kind: .schedule, time: "16:30", text: nil),
        CheckInSpecimenRow(kind: .quiet, time: nil, text: nil),
        CheckInSpecimenRow(kind: .schedule, time: "16:45", text: nil),
        CheckInSpecimenRow(kind: .quiet, time: nil, text: nil),
        CheckInSpecimenRow(kind: .schedule, time: "17:00", text: nil),
        CheckInSpecimenRow(kind: .quiet, time: nil, text: nil),
        CheckInSpecimenRow(kind: .schedule, time: "17:15", text: nil),
        CheckInSpecimenRow(kind: .quiet, time: nil, text: nil),
        CheckInSpecimenRow(kind: .schedule, time: "17:30", text: nil),
        CheckInSpecimenRow(kind: .reply, time: "17:31", text: "Two PRs landed overnight: #412 and #415, both green."),
        CheckInSpecimenRow(kind: .quiet, time: nil, text: nil),
        CheckInSpecimenRow(kind: .schedule, time: "17:45", text: nil),
        CheckInSpecimenRow(kind: .quiet, time: nil, text: nil),
        CheckInSpecimenRow(kind: .schedule, time: "18:00", text: nil),
        CheckInSpecimenRow(kind: .quiet, time: nil, text: nil),
    ]
}
