import Foundation

/// #EXP-1732 THE SESSION SCREEN ×4 (`packages/domain-contract/fixtures/session-screen.json`,
/// its `$comment` is the spec; TS reference `src/session-screen.ts`, desktop
/// `domain::session_screen`, Android `domain/SessionScreen.kt`): ONE screen for
/// a chat, a team agent's or the Assistant's conversation and an app's editor
/// session — the header tabs per kind, the gear and the layout rule (phones
/// are always NARROW: `chat` | `tab` | `item`). `SessionScreenTests` replays
/// `cases`, `kindCases`, `subtitleCases` and `statusCases` whole.
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
            "versions": "Versions",
        ]
        public static let settings = "Settings"
        public static let back = "Back"
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
    }

    public enum TestIds {
        public static let tabs = "session-tabs"
        public static let gear = "session-gear"
        public static let beside = "session-beside"
        public static let pane = "session-pane"
        public static let issues = "session-issues"
        public static let sessions = "session-sessions"
    }

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
        case chat, issues, sessions, results, preview, data, versions

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
        public let gear: Bool
        public let wideDefault: Tab
    }

    public static let kinds: [Kind: KindSpec] = [
        .session: KindSpec(tabs: [.chat, .issues, .sessions, .results], gear: false, wideDefault: .chat),
        .agent: KindSpec(tabs: [.chat, .issues, .sessions, .results], gear: true, wideDefault: .chat),
        .assistant: KindSpec(tabs: [.chat, .issues, .sessions, .results], gear: true, wideDefault: .chat),
        .app: KindSpec(tabs: [.chat, .preview, .data, .versions, .sessions, .results], gear: true, wideDefault: .preview),
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

        public init(id: Tab, label: String, count: Int?) {
            self.id = id
            self.label = label
            self.count = count
        }
    }

    public struct Screen: Equatable, Sendable {
        public let tabs: [TabEntry]
        public let active: Tab
        public let gear: Bool
        public let open: Open?
        public let layout: Layout

        public init(tabs: [TabEntry], active: Tab, gear: Bool, open: Open?, layout: Layout) {
            self.tabs = tabs
            self.active = active
            self.gear = gear
            self.open = open
            self.layout = layout
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

    /// Whether the kind's header carries THE gear.
    public static func gear(_ kind: String) -> Bool { spec(kind).gear }

    public static func screen(
        kind: String, wide: Bool, requested: String?, open: String?, counts: Counts
    ) -> Screen {
        let spec = spec(kind)
        let tabs = spec.tabs.map { tab -> TabEntry in
            let value = counted.contains(tab) ? counts.value(tab) : 0
            return TabEntry(id: tab, label: tab.label, count: value > 0 ? value : nil)
        }
        let req = requested.map { legacyTabs[$0] ?? $0 }
        var opened = open.flatMap(Open.init(rawValue:))
        if opened == nil, req == "settings" { opened = .settings }
        if opened == .settings, !spec.gear { opened = nil }
        // ACTIVE FOLLOWS THE OPENED ITEM: a session → Sessions, an issue →
        // Issues, when the kind has that tab (a plan and settings keep it).
        let follows: Tab? = switch opened {
        case .session?: .sessions
        case .issue?: .issues
        default: nil
        }
        let active: Tab = if let follows, spec.tabs.contains(follows) {
            follows
        } else if let req, let tab = Tab(rawValue: req), spec.tabs.contains(tab) {
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
        return Screen(tabs: tabs, active: active, gear: spec.gear, open: opened, layout: layout)
    }

    /// One session of the tree, as `status(_:)` reads it.
    public struct StatusInput: Equatable, Sendable {
        public var status: String
        public var busy: Bool
        public var needsInput: Bool
        /// The tree's root.
        public var root: Bool

        public init(status: String, busy: Bool, needsInput: Bool, root: Bool) {
            self.status = status
            self.busy = busy
            self.needsInput = needsInput
            self.root = root
        }
    }

    /// THE tree's status word (`statusCases`, lowercase, never dropped):
    /// needs input > working > queued (the root) > done (the root ended) > idle.
    public static func status(_ sessions: [StatusInput]) -> String {
        let live = sessions.filter { $0.status != "ended" }
        let root = sessions.first { $0.root }
        if live.contains(where: \.needsInput) { return Copy.Status.needsInput }
        if live.contains(where: \.busy) { return Copy.Status.working }
        if root?.status == "queued" { return Copy.Status.queued }
        if root?.status == "ended" { return Copy.Status.done }
        return Copy.Status.idle
    }

    /// The typed overload a phone screen calls.
    public static func screen(kind: Kind, requested: Tab?, open: Open?, counts: Counts) -> Screen {
        screen(kind: kind.rawValue, wide: false, requested: requested?.rawValue, open: open?.rawValue, counts: counts)
    }
}
