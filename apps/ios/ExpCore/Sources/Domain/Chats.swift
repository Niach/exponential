import Foundation

/// Thread 58cb32df (owner 2026-10-23, #EXP-1373): Chats, the phone app's
/// WhatsApp-like list, the agent bubbles, the context menus ×4, the transcript
/// dividers and the surface card's reply. Pure and byte-locked by
/// `packages/domain-contract/fixtures/chats.json` (its `$comment` holds the
/// rules; TS reference `src/chats.ts`). `ChatsTests` replays every case group.
public enum Chats {
    // MARK: - Fixture constants

    /// Fixture `copy`.
    public enum Copy {
        public static let title = "Chats"
        public enum Tabs {
            public static let board = "Board"
            public static let devices = "Devices"
            public static let chats = "Chats"
            public static let team = "Team"
        }
        public enum Filters {
            public static let all = "All"
            public static let needsYou = "Needs you"
            public static let running = "Running"
            public static let unread = "Unread"
        }
        public enum Preview {
            public static let working = "working…"
            public static let question = "Question: {text}"
            public static let plan = "Plan: {text}"
            public static let you = "You: {text}"
            public static let picture = "Picture"
            public static let surface = "Preview"
            public static let empty = "No messages yet"
        }
        public enum Time {
            public static let yesterday = "Yesterday"
            public static let weekdaysShort = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
            public static let monthsShort = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        }
        public enum Menu {
            public static let stop = "Stop"
            public static let stopAndDelete = "Stop and delete"
            public static let delete = "Delete"
            public static let compact = "Compact conversation"
            public static let newConversation = "New conversation"
            public static let settings = "Agent settings"
        }
        public enum ConfirmDelete {
            public static let title = "Delete this chat?"
            public static let body = "Its messages and sessions are deleted for everyone. Pull requests stay on GitHub."
        }
        public enum Toasts {
            public static let stopFailed = "Couldn't stop the chat"
            public static let deleteFailed = "Couldn't delete the chat"
            public static let stopNotDelivered = "The device didn't get the stop"
        }
        public enum Dividers {
            public static let new = "New conversation"
            public static let compact = "Conversation compacted"
        }
        public static let superseded = "Replaced by a newer plan"
        public enum BoardChips {
            public static let all = "All issues"
            public static let mine = "My issues"
            public static let drafts = "Drafts"
        }
        public enum Team {
            /// `{members}` / `{boards}`.
            public static let meta = "{members} members · {boards} boards"
            public static let `switch` = "Teams"

            public static func metaLine(members: Int, boards: Int) -> String {
                meta.replacingOccurrences(of: "{members}", with: String(members))
                    .replacingOccurrences(of: "{boards}", with: String(boards))
            }
        }
        public static let inbox = "Inbox"
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let chatRow = "chat-row"
        public static let chatFilter = "chat-filter"
        public static let agentBubble = "agent-bubble"
        public static let inboxButton = "chats-inbox"
        public static let tabTeam = "tab-team"
        public static let boardChip = "board-chip"
        public static let surfaceCard = "surface-card"
        public static let divider = "transcript-divider"
    }

    /// Fixture `shell`.
    public enum Shell {
        public enum Tab: String, CaseIterable, Sendable {
            case board, devices, chats, team

            public var label: String {
                switch self {
                case .board: Copy.Tabs.board
                case .devices: Copy.Tabs.devices
                case .chats: Copy.Tabs.chats
                case .team: Copy.Tabs.team
                }
            }

            /// Concept icon (`tabIcons`); the Team tab paints the team avatar.
            public var icon: String? {
                switch self {
                case .board: "nav-issues"
                case .devices: "nav-devices"
                case .chats: "nav-threads"
                case .team: nil
                }
            }
        }

        public static let tabs: [Tab] = Tab.allCases
        public static let tabIcons: [String: String] = [
            "board": "nav-issues", "devices": "nav-devices", "chats": "nav-threads",
        ]
        public enum BoardChip: String, CaseIterable, Sendable {
            case all, mine, drafts

            public var label: String {
                switch self {
                case .all: Copy.BoardChips.all
                case .mine: Copy.BoardChips.mine
                case .drafts: Copy.BoardChips.drafts
                }
            }
        }
        public static let boardChips: [BoardChip] = BoardChip.allCases
        public static let chatsHeader = ["nav-search", "nav-inbox"]
        public static let leadDisc: Double = 52
        public static let bubble: Double = 66
        public static let bubbleRing: Double = 3
    }

    /// Fixture `surfaceCard` (the ENTER animation; motion tokens by name).
    public enum SurfaceEnter {
        public static let replyEvent = "reply"
        public static let surfaceMaxBytes = 65536
        public static let surfaceMaxComponents = 200
        public enum Frame {
            public static let fromOpacity: Double = 0
            public static let fromTranslateY: Double = 12
            public static let fromScale: Double = 0.98
            public static let duration = "slow"
            public static let ease = "decelerate"
        }
        public enum Children {
            public static let fromOpacity: Double = 0
            public static let fromTranslateY: Double = 6
            public static let duration = "standard"
            public static let ease = "decelerate"
            public static let staggerMs: Double = 45
            public static let maxStaggered = 8
            public static let startAfterMs: Double = 90
        }
        public enum Glow {
            public static let fromOpacity: Double = 0.35
            public static let duration = "slow"
            public static let ease = "decelerate"
        }

        /// The delay (ms) of the surface root's `index`-th first-level child:
        /// `startAfterMs` + `staggerMs` × min(index, maxStaggered - 1).
        public static func childDelayMs(_ index: Int) -> Double {
            Children.startAfterMs + Children.staggerMs * Double(min(max(index, 0), Children.maxStaggered - 1))
        }
    }

    /// #EXP-1383 fixture `agentHint`: the agent chat's "Instructions
    /// changed" Dock banner (dock.json kind `banner`, no new component); its
    /// action sends `sends` exactly like the agent menu's New conversation.
    public enum AgentHint {
        public static let id = "instructions"
        public static let icon = "ui-info"
        public static let label = "Instructions changed"
        public static let text = "This conversation still follows the previous instructions"
        public static let tone = "muted"
        public static let action = "Start new conversation"
        public static let sends = "/new"
        /// The Dock row id of the banner (`banner:<id>`).
        public static let rowId = "banner:\(id)"
    }

    /// #EXP-1383 (`agentHint.cases`): the root session fetched its brief
    /// before the agent's instructions last changed. Either stamp missing (or
    /// unreadable) = no hint; equal = current.
    public static func agentInstructionsChanged(sessionStamp: String?, agentStamp: String?) -> Bool {
        guard let sessionStamp, !sessionStamp.isEmpty, let agentStamp, !agentStamp.isEmpty,
              let session = WireTimestamps.parse(sessionStamp),
              let agent = WireTimestamps.parse(agentStamp)
        else { return false }
        return session < agent
    }

    /// Fixture `agentMenu.sends`.
    public static let agentMenuSends: [AgentMenuEntry: String] = [
        .compact: "/compact",
        .newConversation: "/new",
    ]

    // MARK: - Types

    public enum ChatFilter: String, CaseIterable, Sendable, Hashable {
        case all
        case needsYou = "needs_you"
        case running
        case unread
    }

    public static let filters: [ChatFilter] = ChatFilter.allCases

    public enum ChatLead: String, Equatable, Sendable {
        case needsYou = "needs_you"
        case working, open, merged, waiting
    }

    public enum ChatPreviewKind: String, Equatable, Sendable {
        case working, question, plan, you, agent, picture, surface, new, compact, empty
    }

    public struct ChatPreview: Equatable, Sendable {
        public let kind: ChatPreviewKind
        /// The whole line as painted (the prefix included); clients truncate.
        public let text: String

        public init(kind: ChatPreviewKind, text: String) {
            self.kind = kind
            self.text = text
        }
    }

    public struct ChatRow: Equatable, Sendable, Identifiable {
        public let sessionId: String
        public let teamId: String
        public let title: String
        public let deviceId: String?
        /// The partitionThreads group (`needs_you` | `running` | `recent`).
        public let group: String
        public let lead: ChatLead
        public let preview: ChatPreview
        /// Wire text.
        public let activityAt: String
        public let time: String
        /// Agent messages in the ROOT conversation newer than the root's read.
        public let unreadCount: Int
        public let needsYou: Bool
        public let busy: Bool
        public let word: ThreadRowWord?

        public var id: String { sessionId }
    }

    public struct ChatList: Equatable, Sendable {
        public let rows: [ChatRow]
        /// Chip counts (All has none).
        public let counts: [ChatFilter: Int]
    }

    public enum AgentRing: String, Equatable, Sendable {
        case needsYou = "needs_you"
        case unread
    }

    public struct AgentBubble: Equatable, Sendable, Identifiable {
        public let agent: AgentRow
        public let ring: AgentRing?
        public var id: String { agent.agentId }
    }

    public enum ThreadMenuEntry: String, Equatable, Sendable, CaseIterable {
        case stop, stopAndDelete, delete

        public var label: String {
            switch self {
            case .stop: Copy.Menu.stop
            case .stopAndDelete: Copy.Menu.stopAndDelete
            case .delete: Copy.Menu.delete
            }
        }

        /// Concept icon.
        public var icon: String {
            switch self {
            case .stop: "coding-stop"
            case .stopAndDelete, .delete: "ui-delete"
            }
        }

        public var destructive: Bool { self != .stop }
    }

    public enum AgentMenuEntry: String, Equatable, Sendable, CaseIterable {
        case compact, newConversation, stop, settings

        public var label: String {
            switch self {
            case .compact: Copy.Menu.compact
            case .newConversation: Copy.Menu.newConversation
            case .stop: Copy.Menu.stop
            case .settings: Copy.Menu.settings
            }
        }

        /// Concept icon.
        public var icon: String {
            switch self {
            case .compact: "coding-compact"
            case .newConversation: "ui-clean"
            case .stop: "coding-stop"
            case .settings: "nav-settings"
            }
        }
    }

    public enum DividerKind: String, Equatable, Sendable {
        case new, compact

        public var label: String {
            switch self {
            case .new: Copy.Dividers.new
            case .compact: Copy.Dividers.compact
            }
        }

        /// Concept icon.
        public var icon: String {
            switch self {
            case .new: "ui-clean"
            case .compact: "coding-compact"
            }
        }
    }

    // MARK: - Rules

    private static func ms(_ iso: String?) -> Double? {
        guard let iso, !iso.isEmpty, let date = WireTimestamps.parse(iso) else { return nil }
        return date.timeIntervalSince1970 * 1000
    }

    private static let previewCards: Set<String> = ["question", "plan", "ask", "picture", "surface"]

    /// A builtin command line a person sent (`/new`, `/compact …`): the
    /// transcript paints it as a divider, never a bubble.
    public static func commandDivider(_ body: String) -> DividerKind? {
        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        if text == "/new" { return .new }
        if text == "/compact" || text.hasPrefix("/compact ") { return .compact }
        return nil
    }

    private static func replace(_ text: String, _ pattern: String, _ template: String) -> String {
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return text }
        return regex.stringByReplacingMatches(
            in: text, range: NSRange(text.startIndex..., in: text), withTemplate: template
        )
    }

    /// Markdown → one plain line (the list preview's text).
    public static func plainLine(_ markdown: String) -> String {
        var text = replace(markdown, #"!\[[^\]]*\]\([^)]*\)"#, " ")
        text = replace(text, #"\[([^\]]*)\]\([^)]*\)"#, "$1")
        text = text.components(separatedBy: "\n")
            .map { replace($0, #"^\s*(?:#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+)"#, "") }
            .joined(separator: " ")
        text = replace(text, #"\*\*|__|~~|`"#, "")
        text = replace(text, #"(^|\s)[*_](\S)"#, "$1$2")
        text = replace(text, #"(\S)[*_](?=\s|$)"#, "$1")
        text = replace(text, #"\s+"#, " ")
        return text.trimmingCharacters(in: .whitespaces)
    }

    private static func fill(_ template: String, _ text: String) -> String {
        template.replacingOccurrences(of: "{text}", with: text)
    }

    /// The root conversation's preview line (`previewCases`).
    public static func chatPreview(rootId: String, messages: [TreeMessage], needsYou: Bool, busy: Bool) -> ChatPreview {
        let own = messages
            .filter { $0.sessionId == rootId && ($0.authorKind == "user" || $0.authorKind == "agent") }
            .filter { m in
                if let card = m.card { return previewCards.contains(card.kind ?? "") }
                return !m.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            }
            .sorted { a, b in
                let ta = ms(a.createdAt) ?? 0, tb = ms(b.createdAt) ?? 0
                if ta != tb { return ta > tb }
                return a.id > b.id
            }
        if needsYou,
           let waiting = own.first(where: { m in
               m.answeredAt == nil && ["question", "plan", "ask"].contains(m.card?.kind ?? "")
           }),
           let card = waiting.card {
            if card.kind == "plan" {
                return ChatPreview(kind: .plan, text: fill(Copy.Preview.plan, plainLine(card.plan ?? "")))
            }
            return ChatPreview(kind: .question, text: fill(Copy.Preview.question, plainLine(card.question ?? "")))
        }
        if busy { return ChatPreview(kind: .working, text: Copy.Preview.working) }
        guard let newest = own.first else { return ChatPreview(kind: .empty, text: Copy.Preview.empty) }
        if newest.authorKind == "user" {
            if let divider = commandDivider(newest.body) {
                return ChatPreview(kind: divider == .new ? .new : .compact, text: divider.label)
            }
            return ChatPreview(kind: .you, text: fill(Copy.Preview.you, plainLine(newest.body)))
        }
        switch newest.card?.kind {
        case "picture":
            return ChatPreview(kind: .picture, text: Copy.Preview.picture)
        case "surface":
            let title = newest.card?.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            return ChatPreview(kind: .surface, text: title.isEmpty ? Copy.Preview.surface : title)
        case "question", "ask":
            return ChatPreview(kind: .question, text: fill(Copy.Preview.question, plainLine(newest.card?.question ?? "")))
        case "plan":
            return ChatPreview(kind: .plan, text: fill(Copy.Preview.plan, plainLine(newest.card?.plan ?? "")))
        default:
            return ChatPreview(kind: .agent, text: plainLine(newest.body))
        }
    }

    private static let utc: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }()

    private static func pad2(_ n: Int) -> String { n < 10 ? "0\(n)" : "\(n)" }

    /// The trailing time label (`timeCases`): the viewer's local day by
    /// `utcOffsetMinutes`.
    public static func chatTimeLabel(at: String, now: String, utcOffsetMinutes: Int) -> String {
        chatTimeLabel(
            atMs: ms(at) ?? 0, nowMs: ms(now) ?? 0, utcOffsetMinutes: utcOffsetMinutes
        )
    }

    /// The same off epoch milliseconds.
    public static func chatTimeLabel(atMs: Double, nowMs: Double, utcOffsetMinutes: Int) -> String {
        let shift = Double(utcOffsetMinutes) * 60
        let local = Date(timeIntervalSince1970: atMs / 1000 + shift)
        let today = Date(timeIntervalSince1970: nowMs / 1000 + shift)
        let l = utc.dateComponents([.year, .month, .day, .hour, .minute, .weekday], from: local)
        let t = utc.dateComponents([.year, .month, .day], from: today)
        func dayNumber(_ c: DateComponents) -> Int {
            let date = utc.date(from: DateComponents(year: c.year, month: c.month, day: c.day)) ?? Date(timeIntervalSince1970: 0)
            return Int((date.timeIntervalSince1970 / 86_400).rounded())
        }
        let diff = dayNumber(t) - dayNumber(l)
        if diff <= 0 { return "\(pad2(l.hour ?? 0)):\(pad2(l.minute ?? 0))" }
        if diff == 1 { return Copy.Time.yesterday }
        if diff < 7 { return Copy.Time.weekdaysShort[((l.weekday ?? 1) - 1 + 6) % 7] }
        let date = "\(Copy.Time.monthsShort[(l.month ?? 1) - 1]) \(l.day ?? 1)"
        return l.year == t.year ? date : "\(date), \(l.year ?? 0)"
    }

    /// The viewer's current UTC offset (minutes) for `chatTimeLabel`.
    public static func currentUtcOffsetMinutes(_ zone: TimeZone = .current, at date: Date = Date()) -> Int {
        zone.secondsFromGMT(for: date) / 60
    }

    /// Agent messages of the ROOT conversation (event cards excluded) newer
    /// than the viewer's read of the root.
    public static func unreadCount(rootId: String, messages: [TreeMessage], reads: [TreeSessionRead]) -> Int {
        let read = ms(reads.first(where: { $0.sessionId == rootId })?.readAt) ?? -1
        return messages.filter {
            $0.sessionId == rootId && $0.authorKind == "agent" && $0.card?.kind != "event" && (ms($0.createdAt) ?? 0) > read
        }.count
    }

    private static func keep(_ row: ChatRow, _ filter: ChatFilter) -> Bool {
        switch filter {
        case .all: true
        case .needsYou: row.group == ThreadsListGroup.Key.needsYou.rawValue
        case .running: row.group == ThreadsListGroup.Key.running.rawValue
        case .unread: row.unreadCount > 0
        }
    }

    /// The phone Chats list (`listCases`): the ROOT rows of
    /// `partitionThreads` (active team) flattened and sorted by activity desc
    /// (ties by id), each with its lead, preview, time and unread count, then
    /// filtered by the chip.
    public static func chatList(
        groups: [ThreadsListGroup],
        messages: [TreeMessage],
        reads: [TreeSessionRead] = [],
        prStates: [String: [String]] = [:],
        words: [String: ThreadRowWord] = [:],
        filter: ChatFilter,
        now: String,
        utcOffsetMinutes: Int
    ) -> ChatList {
        let all: [ChatRow] = groups.flatMap { group in
            group.rows.filter { $0.depth == 0 }.map { row -> ChatRow in
                let mark = SessionTree.threadRowMarkState(needsYou: row.needsYou, busy: row.busy)
                let lead: ChatLead
                switch mark {
                case .needsYou: lead = .needsYou
                case .working: lead = .working
                default:
                    switch SessionTree.threadRowLeadPr(prStates: prStates[row.sessionId] ?? []) {
                    case .open: lead = .open
                    case .merged: lead = .merged
                    case nil: lead = .waiting
                    }
                }
                return ChatRow(
                    sessionId: row.sessionId,
                    teamId: row.teamId,
                    title: row.title,
                    deviceId: row.deviceId,
                    group: group.key.rawValue,
                    lead: lead,
                    preview: chatPreview(rootId: row.sessionId, messages: messages, needsYou: row.needsYou, busy: row.busy),
                    activityAt: row.activityAt,
                    time: chatTimeLabel(at: row.activityAt, now: now, utcOffsetMinutes: utcOffsetMinutes),
                    unreadCount: unreadCount(rootId: row.sessionId, messages: messages, reads: reads),
                    needsYou: row.needsYou,
                    busy: row.busy,
                    word: words[row.sessionId]
                )
            }
        }
        .sorted { a, b in
            let ta = ms(a.activityAt) ?? 0, tb = ms(b.activityAt) ?? 0
            if ta != tb { return ta > tb }
            return a.sessionId < b.sessionId
        }
        return ChatList(
            rows: all.filter { keep($0, filter) },
            counts: [
                .needsYou: all.filter { keep($0, .needsYou) }.count,
                .running: all.filter { keep($0, .running) }.count,
                .unread: all.filter { keep($0, .unread) }.count,
            ]
        )
    }

    /// A chip's label: its copy, then its count when there is one.
    public static func chatFilterLabel(_ filter: ChatFilter, counts: [ChatFilter: Int]) -> String {
        let label: String
        switch filter {
        case .all: return Copy.Filters.all
        case .needsYou: label = Copy.Filters.needsYou
        case .running: label = Copy.Filters.running
        case .unread: label = Copy.Filters.unread
        }
        let n = counts[filter] ?? 0
        return n > 0 ? "\(label) \(n)" : label
    }

    /// The Chats header's agent bubbles (`bubbleCases`), in `listAgents`
    /// order: amber ring = the viewer's conversation needs them, white ring =
    /// an agent message newer than their read of it, else none.
    public static func agentBubbles(
        agents: [AgentRow],
        sessions: [TreeSession],
        messages: [TreeMessage],
        reads: [TreeSessionRead] = [],
        me: String?
    ) -> [AgentBubble] {
        agents.map { agent in
            if agent.needsYou { return AgentBubble(agent: agent, ring: .needsYou) }
            let conversation = sessions.first {
                $0.parentId == nil && $0.agentId == agent.agentId && me != nil && $0.userId == me
            }
            let unread = conversation.map { unreadCount(rootId: $0.id, messages: messages, reads: reads) > 0 } ?? false
            return AgentBubble(agent: agent, ring: unread ? .unread : nil)
        }
    }

    /// A chat row's context menu ×4 (`menuCases`): live → Stop, Stop and
    /// delete; ended → Delete; nothing for a viewer who cannot steer it. The
    /// delete entries need `canDelete` (the chat's starter or a team owner,
    /// the server's `sessions.delete` rule).
    public static func threadMenuEntries(rootStatus: String, canSteer: Bool, canDelete: Bool) -> [ThreadMenuEntry] {
        guard canSteer else { return [] }
        if rootStatus == "ended" { return canDelete ? [.delete] : [] }
        return canDelete ? [.stop, .stopAndDelete] : [.stop]
    }

    /// The agent chat's header menu (and its bubble's long-press) ×4
    /// (`agentMenuCases`).
    public static func agentMenuEntries(busy: Bool, isOwner: Bool) -> [AgentMenuEntry] {
        var entries: [AgentMenuEntry] = [.compact, .newConversation]
        if busy { entries.append(.stop) }
        if isOwner { entries.append(.settings) }
        return entries
    }

    /// A press inside an inline preview (`replyCases`): an event named
    /// `reply` with a non-empty `context.text` posts that text as the
    /// person's message; anything else does nothing.
    public static func surfaceReply(name: String?, context: Any?) -> String? {
        guard name == SurfaceEnter.replyEvent else { return nil }
        let text: String
        switch context {
        case let object as [String: Any]:
            text = (object["text"] as? String) ?? ""
        case let json as CardJSON:
            if case let .object(object) = json, case let .string(value)? = object["text"] { text = value } else { text = "" }
        default:
            text = ""
        }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}
