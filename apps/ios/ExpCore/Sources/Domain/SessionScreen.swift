import Foundation

/// #EXP-1732 THE SESSION SCREEN ×4 (`packages/domain-contract/fixtures/session-screen.json`,
/// its `$comment` is the spec; TS reference `src/session-screen.ts`, desktop
/// `domain::session_screen`, Android `domain/SessionScreen.kt`): ONE screen for
/// a chat, a team agent's or the Assistant's conversation and an app's editor
/// session — the header tabs per kind, THE `…` menu (#FEED-239: no gear) and
/// the layout rule (phones are NARROW: `chat` | `tab` | `item`; an iPad's
/// regular width is WIDE: `beside`). `SessionScreenTests` replays `cases`,
/// `menuCases`, `kindCases`, `subtitleCases` and `statusCases` whole.
public enum SessionScreen {
    // MARK: - Fixture constants

    public enum Copy {
        public static let tabs: [String: String] = [
            "chat": "Chat",
            "issues": "Issues",
            "sessions": "Sessions",
            "results": "Results",
            "preview": "Preview",
            "data": "Data",
            "events": "Events",
            "versions": "Versions",
        ]
        public static let settings = "Settings"
        public static let back = "Back"
        /// #FEED-239: THE collapse button's label (wide, the right pane).
        public static let collapse = "Collapse"
        public static let separator = " · "
        public static let linkIssue = "Link or file an issue"
        /// The subtitle's lowercase status words (`status(_:)`).
        public enum Status {
            public static let needsInput = "needs input"
            public static let working = "working"
            public static let queued = "queued"
            public static let done = "done"
            public static let idle = "idle"
        }
        /// #FEED-239: THE `…` menu's labels (settings and delete by kind).
        public enum Menu {
            /// #EXP-1834: THE link menu's way in (Link an issue · New issue).
            public static let linkIssue = "Link or file an issue"
            public static let publish = "Publish"
            public static let compact = "Compact conversation"
            public static let newConversation = "New conversation"
            public static let settings: [String: String] = [
                "agent": "Agent settings",
                "assistant": "Agent settings",
                "app": "App settings",
            ]
            public static let export = "Export"
            public static let delete: [String: String] = [
                "session": "Delete",
                "app": "Delete app",
            ]
        }
    }

    public enum TestIds {
        public static let tabs = "session-tabs"
        public static let beside = "session-beside"
        public static let pane = "session-pane"
        public static let issues = "session-issues"
        public static let sessions = "session-sessions"
        public static let collapse = "session-collapse"
        public static let menu = "session-menu"
    }

    /// #FEED-239: the `…` entries' icon concepts (`menuIcons`).
    public static let menuIcons: [String: String] = [
        "linkIssue": "ui-link",
        "publish": "ui-submit",
        "compact": "coding-compact",
        "newConversation": "ui-clean",
        "settings": "nav-settings",
        "export": "ui-share",
        "delete": "ui-delete",
    ]

    /// #FEED-239: what Compact / New conversation SEND (`menuSends`), the
    /// same path as a typed line.
    public static let menuSends: [String: String] = [
        "compact": "/compact",
        "newConversation": "/new",
    ]

    /// The wide beside rule's geometry (phones never use it; kept for the
    /// constants check).
    public enum Beside {
        public static let breakpoint: Double = 768
        public static let chat: Double = 5
        public static let pane: Double = 7
        public static let chatMinWidth: Double = 360
    }

    public enum Kind: String, Sendable, CaseIterable {
        case session, agent, assistant, app
    }

    public enum Tab: String, Sendable, CaseIterable, Hashable {
        case chat, issues, sessions, results, preview, data, events, versions

        public var label: String { Copy.tabs[rawValue] ?? rawValue }
    }

    public enum Open: String, Sendable, CaseIterable {
        case issue, session, plan, settings
    }

    public enum Layout: String, Sendable {
        case chat, beside, tab, item
    }

    public struct KindSpec: Equatable, Sendable {
        public let tabs: [Tab]
        /// The kind has a settings item (the `…`'s settings entry opens it).
        public let settings: Bool
        /// #FEED-239: false = WIDE drops the Chat segment (the chat is the
        /// pinned left pane).
        public let wideChatTab: Bool
        public let wideDefault: Tab
    }

    public static let kinds: [Kind: KindSpec] = [
        .session: KindSpec(tabs: [.chat, .issues, .sessions, .results], settings: false, wideChatTab: true, wideDefault: .chat),
        .agent: KindSpec(tabs: [.chat, .issues, .sessions, .results], settings: true, wideChatTab: true, wideDefault: .chat),
        .assistant: KindSpec(tabs: [.chat, .issues, .sessions, .results], settings: true, wideChatTab: true, wideDefault: .chat),
        .app: KindSpec(tabs: [.chat, .preview, .data, .events, .versions], settings: true, wideChatTab: false, wideDefault: .preview),
    ]

    /// The tabs that carry a count.
    public static let counted: [Tab] = [.issues, .sessions, .results]

    /// Legacy `?tab=` values (the retired strip's pages).
    public static let legacyTabs: [String: String] = [
        "conversation": "chat",
        "issue": "issues",
        "session": "sessions",
        "guide": "results",
        "stories": "results",
        "plan": "chat",
    ]

    // MARK: - Input / output

    public struct Counts: Equatable, Sendable {
        public var issues: Int
        public var sessions: Int
        public var results: Int

        public init(issues: Int = 0, sessions: Int = 0, results: Int = 0) {
            self.issues = issues
            self.sessions = sessions
            self.results = results
        }

        func value(_ tab: Tab) -> Int {
            switch tab {
            case .issues: issues
            case .sessions: sessions
            case .results: results
            default: 0
            }
        }
    }

    public struct TabEntry: Equatable, Sendable, Identifiable, Hashable {
        public let id: Tab
        public let label: String
        /// A muted number after the label; nil = none (0 included).
        public let count: Int?
        /// #EXP-1834: an empty counted tab: muted, never opened.
        public let disabled: Bool

        public init(id: Tab, label: String, count: Int?, disabled: Bool = false) {
            self.id = id
            self.label = label
            self.count = count
            self.disabled = disabled
        }
    }

    public struct Screen: Equatable, Sendable {
        public let tabs: [TabEntry]
        public let active: Tab
        /// The kind has a settings item (the `…`'s settings entry opens it).
        public let settings: Bool
        public let open: Open?
        public let layout: Layout
        /// #FEED-239: wide, the right pane leads with THE collapse button.
        public let collapse: Bool

        public init(tabs: [TabEntry], active: Tab, settings: Bool, open: Open?, layout: Layout, collapse: Bool) {
            self.tabs = tabs
            self.active = active
            self.settings = settings
            self.open = open
            self.layout = layout
            self.collapse = collapse
        }
    }

    // MARK: - The rules

    /// The screen's kind from its ROOT session (an app wins over its agent).
    public static func kind(agentId: String?, appId: String?, assistant: Bool) -> Kind {
        if let appId, !appId.isEmpty { return .app }
        if let agentId, !agentId.isEmpty { return assistant ? .assistant : .agent }
        return .session
    }

    /// The header's subtitle: `status · device · trigger`, blanks dropped.
    public static func subtitle(status: String?, device: String?, trigger: String?) -> String {
        [status, device, trigger]
            .map { ($0 ?? "").trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: Copy.separator)
    }

    private static func spec(_ kind: String) -> KindSpec {
        Kind(rawValue: kind).flatMap { kinds[$0] } ?? kinds[.session]!
    }

    /// The kind's tabs (no counts).
    public static func tabs(_ kind: String) -> [Tab] { spec(kind).tabs }

    /// Whether the kind has a settings item (`open: settings`).
    public static func hasSettings(_ kind: String) -> Bool { spec(kind).settings }

    public static func screen(
        kind: String, wide: Bool, requested: String?, open: String?, counts: Counts
    ) -> Screen {
        let spec = spec(kind)
        // #EXP-1834: a counted tab with nothing in it is disabled.
        func empty(_ tab: Tab) -> Bool { counted.contains(tab) && counts.value(tab) <= 0 }
        let shown = wide && !spec.wideChatTab ? spec.tabs.filter { $0 != .chat } : spec.tabs
        let tabs = shown.map { tab -> TabEntry in
            let value = counted.contains(tab) ? counts.value(tab) : 0
            return TabEntry(id: tab, label: tab.label, count: value > 0 ? value : nil, disabled: empty(tab))
        }
        let req = requested.map { legacyTabs[$0] ?? $0 }
        var opened = open.flatMap(Open.init(rawValue:))
        if opened == nil, req == "settings" { opened = .settings }
        if opened == .settings, !spec.settings { opened = nil }
        // ACTIVE FOLLOWS THE OPENED ITEM: a session → Sessions, an issue →
        // Issues, when the kind has that tab (a plan and settings keep it).
        let follows: Tab? = switch opened {
        case .session?: .sessions
        case .issue?: .issues
        default: nil
        }
        let active: Tab = if let follows, spec.tabs.contains(follows) {
            follows
        } else if let req, let tab = Tab(rawValue: req), spec.tabs.contains(tab), !empty(tab) {
            tab
        } else {
            wide ? spec.wideDefault : .chat
        }
        let layout: Layout = if wide {
            active != .chat || opened != nil ? .beside : .chat
        } else if opened != nil {
            .item
        } else {
            active == .chat ? .chat : .tab
        }
        return Screen(
            tabs: tabs, active: active, settings: spec.settings, open: opened, layout: layout, collapse: layout == .beside
        )
    }

    /// One session of the tree, as `status(_:)` reads it.
    public struct StatusInput: Equatable, Sendable {
        /// #FEED-257: the session's id.
        public var id: String
        public var status: String
        public var busy: Bool
        public var needsInput: Bool
        /// #FEED-257: its `blocked.kind` (a wall or a watchdog flag; nil = none).
        public var blocked: String?
        /// The tree's root.
        public var root: Bool

        public init(id: String = "", status: String, busy: Bool, needsInput: Bool, blocked: String? = nil, root: Bool) {
            self.id = id
            self.status = status
            self.busy = busy
            self.needsInput = needsInput
            self.blocked = blocked
            self.root = root
        }
    }

    /// #FEED-257: one waiting card of the chat (`chatPendingAsks`,
    /// Conversation order).
    public struct Ask: Equatable, Sendable {
        public enum Kind: String, Sendable { case plan, question }
        public var id: String
        public var sessionId: String
        public var kind: Kind

        public init(id: String, sessionId: String, kind: Kind) {
            self.id = id
            self.sessionId = sessionId
            self.kind = kind
        }
    }

    /// #FEED-257: what the needs-input line opens.
    public struct Waiting: Equatable, Sendable {
        public enum Open: String, Sendable { case plan, card, session, chat }
        public var open: Open
        public var id: String

        public init(open: Open, id: String) {
            self.open = open
            self.id = id
        }
    }

    /// #FEED-257 (`waitingCases`): THE waiting item: the first ask, else the
    /// first flagged non-root session that waits, else the root's own flag,
    /// else nil (a stale `needsInput` with nothing to open is nothing).
    public static func waiting(_ sessions: [StatusInput], asks: [Ask] = []) -> Waiting? {
        if let ask = asks.first { return Waiting(open: ask.kind == .plan ? .plan : .card, id: ask.id) }
        let flagged = sessions.filter { $0.status != "ended" && $0.needsInput && $0.blocked != nil }
        if let child = flagged.first(where: { !$0.root }) { return Waiting(open: .session, id: child.id) }
        if let root = flagged.first(where: \.root) { return Waiting(open: .chat, id: root.id) }
        return nil
    }

    /// THE tree's status word (`statusCases`, lowercase, never dropped):
    /// needs input (#FEED-257: only while something waits, `waiting`) >
    /// working > queued (the root) > done (the root ended) > idle.
    public static func status(_ sessions: [StatusInput], asks: [Ask] = []) -> String {
        let live = sessions.filter { $0.status != "ended" }
        let root = sessions.first { $0.root }
        if waiting(sessions, asks: asks) != nil { return Copy.Status.needsInput }
        if live.contains(where: \.busy) { return Copy.Status.working }
        if root?.status == "queued" { return Copy.Status.queued }
        if root?.status == "ended" { return Copy.Status.done }
        return Copy.Status.idle
    }

    /// The typed overload a screen calls (phones: `wide` false).
    public static func screen(kind: Kind, wide: Bool = false, requested: Tab?, open: Open?, counts: Counts) -> Screen {
        screen(kind: kind.rawValue, wide: wide, requested: requested?.rawValue, open: open?.rawValue, counts: counts)
    }

    // MARK: - THE `…` menu (#FEED-239)

    public enum MenuEntry: String, Sendable, CaseIterable, Hashable {
        case linkIssue, publish, compact, newConversation, settings, export, delete, separator

        /// The entry's icon concept (`menuIcons`); nil for the separator.
        public var iconConcept: String? { SessionScreen.menuIcons[rawValue] }

        /// What the entry SENDS into the conversation (`menuSends`), if any.
        public var sends: String? { SessionScreen.menuSends[rawValue] }

        public var destructive: Bool { self == .delete }
    }

    /// THE `…` of every kind (`menuCases`): groups in order, a `separator`
    /// between non-empty ones: link an issue (not an app, #EXP-1834) ·
    /// publish (an app on a phone) · compact + new conversation (not a chat)
    /// · settings · export + delete.
    public static func menu(
        kind: String, phone: Bool, member: Bool, linkIssue: Bool, settings: Bool, publish: Bool, export: Bool,
        delete: Bool
    ) -> [MenuEntry] {
        let spec = spec(kind)
        let app = kind == Kind.app.rawValue
        let conversation = kind != Kind.session.rawValue && Kind(rawValue: kind) != nil
        let links = kind == Kind.session.rawValue || kind == Kind.agent.rawValue || kind == Kind.assistant.rawValue
        var last: [MenuEntry] = []
        if app, export { last.append(.export) }
        if delete { last.append(.delete) }
        let groups: [[MenuEntry]] = [
            links && linkIssue ? [.linkIssue] : [],
            app && phone && publish ? [.publish] : [],
            conversation && member ? [.compact, .newConversation] : [],
            spec.settings && settings ? [.settings] : [],
            last,
        ]
        var out: [MenuEntry] = []
        for group in groups where !group.isEmpty {
            if !out.isEmpty { out.append(.separator) }
            out.append(contentsOf: group)
        }
        return out
    }

    /// The typed overload.
    public static func menu(
        kind: Kind, phone: Bool, member: Bool, linkIssue: Bool, settings: Bool, publish: Bool, export: Bool,
        delete: Bool
    ) -> [MenuEntry] {
        menu(
            kind: kind.rawValue, phone: phone, member: member, linkIssue: linkIssue, settings: settings,
            publish: publish, export: export, delete: delete
        )
    }

    /// A menu entry's label for the kind (settings and delete read by kind).
    public static func menuLabel(kind: String, entry: MenuEntry) -> String {
        switch entry {
        case .linkIssue: Copy.Menu.linkIssue
        case .publish: Copy.Menu.publish
        case .compact: Copy.Menu.compact
        case .newConversation: Copy.Menu.newConversation
        case .settings: Copy.Menu.settings[kind] ?? Copy.Menu.settings["agent"] ?? ""
        case .export: Copy.Menu.export
        case .delete: Copy.Menu.delete[kind] ?? Copy.Menu.delete["session"] ?? ""
        case .separator: ""
        }
    }
}
