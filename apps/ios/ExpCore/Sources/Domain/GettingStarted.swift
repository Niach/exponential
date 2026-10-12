import Foundation

/// EXP-1270 (EXP-1259 D6): Getting started ×4 = ONE block — a header (title,
/// `N of M`, a meter, Docs) over ONE row of square cards, the next step
/// highlighted, done steps checked and still tappable, no locked state. The
/// pure model `deriveGettingStarted(signals, permissions, platform)` ported
/// from web `components/getting-started/getting-started-model.ts` (desktop
/// `getting_started.rs`, Android `GettingStartedModel.kt`), locked against
/// `packages/domain-contract/fixtures/getting-started.json` by
/// `GettingStartedTests` (every case, whole objects).
public enum GettingStarted {
    public enum Key: String, CaseIterable, Sendable {
        case board, github, device, run, invite, server, mcp, widget
    }

    /// What a card's tap runs (`targets` in the fixture's `$comment`).
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
        /// The card's ONE line of copy (fixture `steps[].title`).
        public let title: String
        /// Concept id (`packages/icons/icons.json`).
        public let icon: String
        public let target: Target
        public let needsDevice: Bool
        public let visibleWhen: VisibleWhen?
        public let phones: Bool
    }

    public enum VisibleWhen: String, Sendable {
        case canManageMembers, canManageWidgets
    }

    /// The eight steps in display order (fixture `steps`).
    public static let steps: [Step] = [
        Step(key: .board, title: "Create a board", icon: "ui-folder", target: .createBoard, needsDevice: false, visibleWhen: nil, phones: true),
        Step(key: .github, title: "Connect GitHub", icon: "ui-github", target: .github, needsDevice: false, visibleWhen: nil, phones: true),
        Step(key: .device, title: "Add a device", icon: "ui-device", target: .addDevice, needsDevice: false, visibleWhen: nil, phones: true),
        Step(key: .run, title: "Start a session", icon: "action-run", target: .agent, needsDevice: true, visibleWhen: nil, phones: true),
        Step(key: .invite, title: "Invite your team", icon: "ui-invite", target: .invite, needsDevice: false, visibleWhen: .canManageMembers, phones: true),
        Step(key: .server, title: "Set up a server", icon: "ui-server", target: .addServer, needsDevice: false, visibleWhen: nil, phones: true),
        Step(key: .mcp, title: "Connect your tools", icon: "ui-mcp", target: .mcp, needsDevice: false, visibleWhen: nil, phones: false),
        Step(key: .widget, title: "Feedback widget", icon: "ui-widget", target: .widget, needsDevice: false, visibleWhen: .canManageWidgets, phones: false),
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

    public enum Card {
        public static let width: Double = 104
        public static let height: Double = 112
        public static let padding: Double = 12
        public static let icon: Double = 16
        public static let gap: Double = 8
    }

    public enum TestId {
        public static let block = "getting-started"
        public static let docs = "getting-started-docs"
        public static let entry = "getting-started-entry"
        public static func card(_ key: Key) -> String { "getting-started-card-\(key.rawValue)" }
    }

    public static func progress(done: Int, total: Int) -> String { "\(done) of \(total)" }

    /// The card title for `key`.
    public static func title(_ key: Key) -> String {
        steps.first { $0.key == key }?.title ?? key.rawValue
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
