import Foundation

/// EXP-1249: the launch composer's ONE "+" menu as a pure layout, replayed ×4
/// off `packages/domain-contract/fixtures/composer-menu.json` (web
/// `components/launch-dialog/composer-menu.ts`, desktop `chat_screen.rs`,
/// Android `AgentComposer.kt`): which rows show for the picked agent / device
/// / subject, in what order, with which concept glyph and copy. Phones open it
/// as a bottom sheet, submenus as pushed pages (`ComposerPlusMenu`). The
/// page around the card is `composer-home.json`'s. S-3: EVERY composer
/// (launch, a session's, an agent chat's: `Surface`) has only the "+"; a row
/// exists on its `surfaces` and shows while ALL its `when` conditions hold.
/// The old `message-agent` row is gone (the agent row picks the recipient).
public enum ComposerMenu {
    /// S-3: which composer the "+" belongs to (fixture row `surfaces`, case
    /// `surface`): the launch composer, a session's, an agent chat's.
    public enum Surface: String, Equatable, Sendable, CaseIterable {
        case launch
        case session
        case agent
    }

    public enum RowId: String, Equatable, Sendable, CaseIterable {
        /// "Photo or file" (iOS keeps its Photo | File sub-choice).
        case addFile = "add-file"
        /// "Issue": launch = the implement subject picker; session / agent
        /// = the same issue search list, a pick inserts `#IDENT` at the caret.
        case implementIssue = "implement-issue"
        /// S-3 runner submenus over THE existing pickers.
        case device
        case account
        case model
        case repository
        case effort
        case subagents
        case ultracode
        case mcpServers = "mcp-servers"
        case computerUse = "computer-use"
    }

    public enum Kind: String, Equatable, Sendable {
        case submenu
        case item
        case toggle
    }

    /// One fixture `conditions` key.
    public enum Condition: String, Equatable, Sendable, CaseIterable {
        case temporary, device, account, model, repositories, subagentModel, ultracode, mcp, computerUse
    }

    /// When a conditional row shows (fixture `conditions`).
    public struct Conditions: Equatable, Sendable {
        /// Launch: the agent row (`ComposerAgentRow`) picks Temporary
        /// session (no agent).
        public var temporary: Bool
        /// The device can change here (launch while temporary).
        public var device: Bool
        /// The login can change here (launch while temporary).
        public var account: Bool
        /// The model can change here (launch while temporary; a session or
        /// an agent chat whose agent switches models).
        public var model: Bool
        /// The team has more than one repository.
        public var repositories: Bool
        /// The picked agent takes a subagent model (claude).
        public var subagentModel: Bool
        /// The picked agent supports ultracode (claude).
        public var ultracode: Bool
        /// The team has at least one MCP server.
        public var mcp: Bool
        /// The picked device advertises `computerUseRunCap`.
        public var computerUse: Bool

        public init(
            temporary: Bool = false,
            device: Bool = false,
            account: Bool = false,
            model: Bool = false,
            repositories: Bool = false,
            subagentModel: Bool = false,
            ultracode: Bool = false,
            mcp: Bool = false,
            computerUse: Bool = false
        ) {
            self.temporary = temporary
            self.device = device
            self.account = account
            self.model = model
            self.repositories = repositories
            self.subagentModel = subagentModel
            self.ultracode = ultracode
            self.mcp = mcp
            self.computerUse = computerUse
        }

        public func holds(_ condition: Condition) -> Bool {
            switch condition {
            case .temporary: temporary
            case .device: device
            case .account: account
            case .model: model
            case .repositories: repositories
            case .subagentModel: subagentModel
            case .ultracode: ultracode
            case .mcp: mcp
            case .computerUse: computerUse
            }
        }
    }

    public struct Row: Equatable, Sendable, Identifiable {
        public let id: RowId
        public let kind: Kind
        public let label: String
        /// The codex wording of the row, when it has one ("Reasoning").
        public let codexLabel: String?
        /// A concept id (`packages/icons/icons.json`).
        public let icon: String
        /// Where the row exists.
        public let surfaces: [Surface]
        /// When it shows: every condition must hold ([] = always).
        public let when: [Condition]

        public func label(codex: Bool) -> String { codex ? (codexLabel ?? label) : label }
    }

    public enum Entry: Equatable, Sendable {
        case row(Row)
        case separator
    }

    private static let everywhere: [Surface] = [.launch, .session, .agent]

    /// Fixture `rows` in order; nil = a separator.
    private static let rows: [Row?] = [
        Row(id: .addFile, kind: .item, label: "Photo or file", codexLabel: nil, icon: "ui-attach", surfaces: everywhere, when: []),
        Row(id: .implementIssue, kind: .submenu, label: "Issue", codexLabel: nil, icon: "editor-issue-ref", surfaces: everywhere, when: []),
        nil,
        Row(id: .device, kind: .submenu, label: "Device", codexLabel: nil, icon: "nav-devices", surfaces: everywhere, when: [.device]),
        Row(id: .account, kind: .submenu, label: "Account", codexLabel: nil, icon: "settings-account", surfaces: everywhere, when: [.account]),
        Row(id: .model, kind: .submenu, label: "Model", codexLabel: nil, icon: "coding-assistant", surfaces: everywhere, when: [.model]),
        Row(id: .repository, kind: .submenu, label: "Repository", codexLabel: nil, icon: "settings-repositories", surfaces: [.launch], when: [.temporary, .repositories]),
        nil,
        Row(id: .effort, kind: .submenu, label: "Effort", codexLabel: "Reasoning", icon: "ui-estimate", surfaces: [.launch], when: [.temporary]),
        Row(id: .subagents, kind: .submenu, label: "Subagents", codexLabel: nil, icon: "coding-subagent", surfaces: [.launch], when: [.temporary, .subagentModel]),
        Row(id: .ultracode, kind: .toggle, label: "Ultracode", codexLabel: nil, icon: "action-default", surfaces: [.launch], when: [.temporary, .ultracode]),
        nil,
        Row(id: .mcpServers, kind: .submenu, label: "MCP servers", codexLabel: nil, icon: "ui-mcp", surfaces: [.launch], when: [.temporary, .mcp]),
        Row(id: .computerUse, kind: .toggle, label: "Computer use", codexLabel: nil, icon: "nav-computer", surfaces: [.launch], when: [.temporary, .computerUse]),
    ]

    public static let plusTestId = "agent-composer-plus-button"
    public static let menuTestId = "agent-composer-menu"
    public static let implementSubmitTestId = "agent-composer-implement"
    /// SLOP-28: the submenu pages' search input (the shared search list).
    public static let submenuSearchTestId = "agent-composer-submenu-search"
    public static let plusLabel = "Add"
    /// SLOP-28 (EXP-1259 D5): Implement issue is the shared search list
    /// (`SearchList`): one row height, at most this many visible
    /// on a pointer (phones push them as pages of 44pt rows).
    public static let submenuRowHeight: Double = 32
    public static let submenuMaxRows = 8
    /// The per-run computer-use flag's start-payload key and the device
    /// capability that reads it (contract `codingSession`).
    public static let computerUsePayloadKey = "computerUse"
    public static let computerUseRunCap = DomainContract.codingSessionComputerUseCap

    public static func rowTestId(_ id: RowId) -> String { "agent-composer-menu-\(id.rawValue)" }

    /// Every row in fixture order, ignoring surfaces and conditions (what
    /// the glyph and copy tests read).
    public static var allRows: [Row] { rows.compactMap { $0 } }

    public static func row(_ id: RowId) -> Row { allRows.first { $0.id == id }! }

    /// The rows the "+" of `surface` shows under `conditions`, separators
    /// tidied (never leading, trailing or doubled).
    public static func composerMenuLayout(_ conditions: Conditions, surface: Surface) -> [Entry] {
        var out: [Entry] = []
        for row in rows {
            guard let row else {
                if let last = out.last, last != .separator { out.append(.separator) }
                continue
            }
            guard row.surfaces.contains(surface), row.when.allSatisfy(conditions.holds) else { continue }
            out.append(.row(row))
        }
        while out.last == .separator { out.removeLast() }
        return out
    }

    /// The Implement submenu's footer button once something is picked.
    public static func implementButtonLabel(_ count: Int) -> String {
        count == 1 ? "Implement 1 issue" : "Implement \(count) issues"
    }
}
