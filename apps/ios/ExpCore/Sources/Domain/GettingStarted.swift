import Foundation

/// EXP-1270 (EXP-1259 D6): Getting started ×4 = ONE block — a group band
/// (title, `N of M`, a meter, Docs) over flat hairline-divided ROWS, one per
/// visible step: done rows muted + checked and still tappable, the ONE next
/// row expanded (its `why` + ONE primary `cta` button), later rows plain with
/// a quiet chevron; no numbers, no locked state. The
/// pure model `deriveGettingStarted(signals, permissions, platform)` ported
/// from web `components/getting-started/getting-started-model.ts` (desktop
/// `getting_started.rs`, Android `GettingStartedModel.kt`), locked against
/// `packages/domain-contract/fixtures/getting-started.json` by
/// `GettingStartedTests` (every case, whole objects).
public enum GettingStarted {
    public enum Key: String, CaseIterable, Sendable {
        case board, github, device, run, invite, server, mcp, widget
    }

    /// What a row's tap runs (`targets` in the fixture's `$comment`).
    public enum Target: String, Sendable {
        case createBoard = "create-board"
        case github
        case addDevice = "add-device"
        case agent
        case invite
        case addServer = "add-server"
        case mcp
        case widget
    }

    public enum Platform: String, Sendable {
        case pointer
        case phone
    }

    public struct Signals: Equatable, Sendable {
        public var hasBoard = false
        public var githubInstalled = false
        public var hasDesktopDevice = false
        public var hasServerDevice = false
        public var hasCodingSession = false
        public var hasInvitedTeam = false
        public var mcpConnected = false
        public var hasWidget = false

        public init(
            hasBoard: Bool = false,
            githubInstalled: Bool = false,
            hasDesktopDevice: Bool = false,
            hasServerDevice: Bool = false,
            hasCodingSession: Bool = false,
            hasInvitedTeam: Bool = false,
            mcpConnected: Bool = false,
            hasWidget: Bool = false
        ) {
            self.hasBoard = hasBoard
            self.githubInstalled = githubInstalled
            self.hasDesktopDevice = hasDesktopDevice
            self.hasServerDevice = hasServerDevice
            self.hasCodingSession = hasCodingSession
            self.hasInvitedTeam = hasInvitedTeam
            self.mcpConnected = mcpConnected
            self.hasWidget = hasWidget
        }
    }

    public struct Permissions: Equatable, Sendable {
        public var canManageMembers: Bool
        public var canManageWidgets: Bool

        public init(canManageMembers: Bool, canManageWidgets: Bool) {
            self.canManageMembers = canManageMembers
            self.canManageWidgets = canManageWidgets
        }
    }

    public struct Step: Equatable, Sendable {
        public let key: Key
        /// The row's title (fixture `steps[].title`).
        public let title: String
        /// Concept id (`packages/icons/icons.json`).
        public let icon: String
        public let target: Target
        public let needsDevice: Bool
        public let visibleWhen: VisibleWhen?
        public let phones: Bool
        /// The expanded (next) row's ONE sentence.
        public let why: String
        /// The expanded row's primary button label.
        public let cta: String
    }

    public enum VisibleWhen: String, Sendable {
        case canManageMembers, canManageWidgets
    }

    /// The eight steps in display order (fixture `steps`).
    public static let steps: [Step] = [
        Step(key: .board, title: "Create a board", icon: "ui-folder", target: .createBoard, needsDevice: false, visibleWhen: nil, phones: true,
             why: "A board holds the issues your agents work on.",
             cta: "Create a board"),
        Step(key: .github, title: "Connect GitHub", icon: "ui-github", target: .github, needsDevice: false, visibleWhen: nil, phones: true,
             why: "Sessions branch, push and open pull requests on your repositories.",
             cta: "Connect GitHub"),
        Step(key: .device, title: "Add a device", icon: "ui-device", target: .addDevice, needsDevice: false, visibleWhen: nil, phones: true,
             why: "Sessions run on your computer. The desktop app registers it.",
             cta: "Add a device"),
        Step(key: .run, title: "Start a session", icon: "action-run", target: .agent, needsDevice: true, visibleWhen: nil, phones: true,
             why: "Pick an issue or type what to build, then watch it work.",
             cta: "Start a session"),
        Step(key: .invite, title: "Invite your team", icon: "ui-invite", target: .invite, needsDevice: false, visibleWhen: .canManageMembers, phones: true,
             why: "Teammates see every board, session and review.",
             cta: "Invite your team"),
        Step(key: .server, title: "Set up a server", icon: "ui-server", target: .addServer, needsDevice: false, visibleWhen: nil, phones: true,
             why: "An always-on machine takes sessions while your laptop is closed.",
             cta: "Set up a server"),
        Step(key: .mcp, title: "Connect your tools", icon: "ui-mcp", target: .mcp, needsDevice: false, visibleWhen: nil, phones: false,
             why: "Claude, ChatGPT or Cursor read and file issues through MCP.",
             cta: "Connect your tools"),
        Step(key: .widget, title: "Feedback widget", icon: "ui-widget", target: .widget, needsDevice: false, visibleWhen: .canManageWidgets, phones: false,
             why: "A snippet on your site turns feedback into issues.",
             cta: "Feedback widget"),
    ]

    public struct Entry: Equatable, Sendable, Identifiable {
        public let key: Key
        public let done: Bool
        /// The step's target, or `add-device` for a step that needs a device
        /// while there is none.
        public let target: Target
        /// The first step not done.
        public let next: Bool
        public var id: String { key.rawValue }

        public init(key: Key, done: Bool, target: Target, next: Bool) {
            self.key = key
            self.done = done
            self.target = target
            self.next = next
        }
    }

    public struct State: Equatable, Sendable {
        public let entries: [Entry]
        public let done: Int
        public let total: Int
        /// `{done} of {total}`.
        public let progress: String
        /// Every visible step is done: the block (and its entry) disappear.
        public let complete: Bool
    }

    public static let title = "Getting started"
    public static let docsLabel = "Docs"
    public static let docsIcon = "ui-docs"
    public static let docsUrl = URL(string: "https://exponential.at/docs/getting-started/")!
    /// The header's progress meter width.
    public static let meterWidth: Double = 160
    /// The small label trailing the expanded (next) row's title.
    public static let nextLabel = "Next"
    /// The block's widest (the pointer dialog's width); phones fill the sheet.
    public static let maxWidth: Double = 560

    /// The row geometry (fixture `row`).
    public enum Row {
        /// A row's least height on a pointer client.
        public static let height: Double = 44
        /// A row's least height on a phone.
        public static let heightPhone: Double = 48
        public static let iconSize: Double = 16
        /// Horizontal padding.
        public static let padding: Double = 16
        /// The expanded row's vertical padding.
        public static let expandedPadding: Double = 14
    }

    public enum TestId {
        public static let block = "getting-started"
        public static let docs = "getting-started-docs"
        public static let entry = "getting-started-entry"
        public static func row(_ key: Key) -> String { "getting-started-row-\(key.rawValue)" }
        public static let cta = "getting-started-cta"
    }

    public static func progress(done: Int, total: Int) -> String { "\(done) of \(total)" }

    /// The row title for `key`.
    public static func title(_ key: Key) -> String {
        steps.first { $0.key == key }?.title ?? key.rawValue
    }

    /// The expanded row's sentence for `key`.
    public static func why(_ key: Key) -> String {
        steps.first { $0.key == key }?.why ?? ""
    }

    /// The expanded row's button label for `key`.
    public static func cta(_ key: Key) -> String {
        steps.first { $0.key == key }?.cta ?? title(key)
    }

    static func isDone(_ key: Key, _ s: Signals) -> Bool {
        switch key {
        case .board: s.hasBoard
        case .github: s.githubInstalled
        case .device: s.hasDesktopDevice || s.hasServerDevice
        case .run: s.hasCodingSession
        case .invite: s.hasInvitedTeam
        case .server: s.hasServerDevice
        case .mcp: s.mcpConnected
        case .widget: s.hasWidget
        }
    }

    /// The block's entries for `signals` as `permissions` and `platform` see it.
    public static func derive(_ signals: Signals, permissions: Permissions, platform: Platform) -> State {
        let hasDevice = signals.hasDesktopDevice || signals.hasServerDevice
        let visible = steps.filter { step in
            if platform == .phone && !step.phones { return false }
            switch step.visibleWhen {
            case .canManageMembers: return permissions.canManageMembers
            case .canManageWidgets: return permissions.canManageWidgets
            case nil: return true
            }
        }
        var nextAssigned = false
        let entries = visible.map { step -> Entry in
            let done = isDone(step.key, signals)
            let next = !done && !nextAssigned
            if next { nextAssigned = true }
            return Entry(
                key: step.key,
                done: done,
                target: step.needsDevice && !hasDevice ? .addDevice : step.target,
                next: next
            )
        }
        let done = entries.filter(\.done).count
        return State(
            entries: entries,
            done: done,
            total: entries.count,
            progress: progress(done: done, total: entries.count),
            complete: !entries.isEmpty && done == entries.count
        )
    }
}
