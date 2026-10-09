import Foundation

/// EXP-1249: the launch composer's ONE "+" menu as a pure layout, replayed ×4
/// off `packages/domain-contract/fixtures/composer-menu.json` (web
/// `components/launch-dialog/composer-menu.ts`, desktop `chat_screen.rs`,
/// Android `AgentComposer.kt`): which rows show for the picked agent / device
/// / subject, in what order, with which concept glyph and copy. Phones open it
/// as a bottom sheet, submenus as pushed pages.
public enum ComposerMenu {
    public enum RowId: String, Equatable, Sendable, CaseIterable {
        case implementIssue = "implement-issue"
        case runAction = "run-action"
        case addFile = "add-file"
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

    /// When a conditional row shows (fixture `conditions`).
    public struct Conditions: Equatable, Sendable {
        /// The picked agent takes a subagent model (claude).
        public var subagentModel: Bool
        /// The picked agent supports ultracode (claude).
        public var ultracode: Bool
        /// The team has an MCP server and the subject does not own its list.
        public var mcp: Bool
        /// The picked device advertises `computerUseRunCap`.
        public var computerUse: Bool

        public init(subagentModel: Bool, ultracode: Bool, mcp: Bool, computerUse: Bool) {
            self.subagentModel = subagentModel
            self.ultracode = ultracode
            self.mcp = mcp
            self.computerUse = computerUse
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

        public func label(codex: Bool) -> String { codex ? (codexLabel ?? label) : label }
    }

    public enum Entry: Equatable, Sendable {
        case row(Row)
        case separator
    }

    private enum When {
        case subagentModel, ultracode, mcp, computerUse
    }

    private static let rows: [(Row?, When?)] = [
        (Row(id: .implementIssue, kind: .submenu, label: "Implement issue", codexLabel: nil, icon: "editor-issue-ref"), nil),
        (Row(id: .runAction, kind: .submenu, label: "Run action", codexLabel: nil, icon: "action-run"), nil),
        (Row(id: .addFile, kind: .item, label: "Add file or image", codexLabel: nil, icon: "ui-attach"), nil),
        (nil, nil),
        (Row(id: .effort, kind: .submenu, label: "Effort", codexLabel: "Reasoning", icon: "ui-estimate"), nil),
        (Row(id: .subagents, kind: .submenu, label: "Subagents", codexLabel: nil, icon: "coding-subagent"), .subagentModel),
        (Row(id: .ultracode, kind: .toggle, label: "Ultracode", codexLabel: nil, icon: "action-default"), .ultracode),
        (nil, nil),
        (Row(id: .mcpServers, kind: .submenu, label: "MCP servers", codexLabel: nil, icon: "ui-mcp"), .mcp),
        (Row(id: .computerUse, kind: .toggle, label: "Computer use", codexLabel: nil, icon: "nav-computer"), .computerUse),
    ]

    public static let plusTestId = "agent-composer-plus-button"
    public static let menuTestId = "agent-composer-menu"
    public static let implementSubmitTestId = "agent-composer-implement"
    public static let suggestionTestId = "agent-composer-suggestion"
    public static let brandMarkTestId = "agent-page-brand-mark"
    public static let plusLabel = "Add"
    /// How many chat suggestions the quiet rows under the composer show.
    public static let suggestionCount = 3
    public static let suggestionIcon = "action-default"
    /// The per-run computer-use flag's start-payload key and the device
    /// capability that reads it (contract `codingSession`).
    public static let computerUsePayloadKey = "computerUse"
    public static let computerUseRunCap = DomainContract.codingSessionComputerUseCap

    public static func rowTestId(_ id: RowId) -> String { "agent-composer-menu-\(id.rawValue)" }

    /// Every row in fixture order, ignoring conditions (what the glyph and
    /// copy tests read).
    public static var allRows: [Row] { rows.compactMap(\.0) }

    /// The rows the "+" shows under `conditions`, separators tidied (never
    /// leading, trailing or doubled).
    public static func composerMenuLayout(_ conditions: Conditions) -> [Entry] {
        var out: [Entry] = []
        for (row, when) in rows {
            guard let row else {
                if let last = out.last, last != .separator { out.append(.separator) }
                continue
            }
            if let when {
                let holds = switch when {
                case .subagentModel: conditions.subagentModel
                case .ultracode: conditions.ultracode
                case .mcp: conditions.mcp
                case .computerUse: conditions.computerUse
                }
                if !holds { continue }
            }
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
