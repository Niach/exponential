import ExpCore
import SwiftUI
internal import ExponentialUIPrimitives

/// EXP-1274: the ONE empty state on iOS, drawn by the SDK's `EmptyStateView`
/// in the app's glass tokens. Copy comes from `EmptyStateCopy` (locked ×4
/// against `empty-state.json`), so call sites read `EmptyState(.inbox)` or
/// `EmptyState(.boards) { createBoard() }`.
///
/// `page` = the 48pt `fillActive` disc with a 24pt glyph, a `.headline`
/// title, an optional `.subheadline` sentence and an optional primary pill;
/// `list` = a bare 20pt glyph over one `.subheadline` muted line, no
/// description, no button (the section band's pill is the call to action).
public struct EmptyState<Actions: View>: View {
    let size: EmptyStateCopy.Size
    let icon: String?
    let title: String
    let description: String?
    let actions: Actions
    let hasActions: Bool

    public var body: some View {
        content.primitiveTokens(PrimitiveTokens(
            foreground: .white,
            mutedForeground: .white.opacity(TextOpacity.secondary),
            muted: GlassTokens.fillActive
        ))
    }

    private var sdkSize: EmptyStateSize { size == .page ? .page : .list }
    private static var fonts: EmptyStateFonts {
        EmptyStateFonts(title: .headline, description: .subheadline, listTitle: .subheadline)
    }

    @ViewBuilder
    private var glyph: some View {
        if let icon { AppIcon(icon, size: sdkSize.glyph) }
    }

    @ViewBuilder
    private var content: some View {
        if hasActions && size == .page {
            EmptyStateView(title: title, description: description, size: .page, fonts: Self.fonts) {
                glyph
            } actions: {
                actions
            }
        } else {
            EmptyStateView(
                title: title,
                description: size == .page ? description : nil,
                size: sdkSize,
                fonts: Self.fonts
            ) {
                glyph
            }
        }
    }
}

extension EmptyState {
    /// A contract state with the host's own actions row (a pill that spins
    /// while it prepares, two buttons).
    public init(
        _ state: EmptyStateCopy.State,
        description: String? = nil,
        @ViewBuilder actions: () -> Actions
    ) {
        self.size = state.size
        self.icon = EmptyStateIcon.glyph(state.icon)
        self.title = state.title
        self.description = description ?? state.description
        self.actions = actions()
        self.hasActions = true
    }

    /// Inline copy another contract locks with the host's own actions row
    /// (VAPP-176: a draft app's starter pills); actions draw on `page` only.
    public init(
        title: String,
        description: String? = nil,
        icon: String? = nil,
        size: EmptyStateCopy.Size = .page,
        @ViewBuilder actions: () -> Actions
    ) {
        self.size = size
        self.icon = icon
        self.title = title
        self.description = description
        self.actions = actions()
        self.hasActions = true
    }
}

extension EmptyState where Actions == EmptyStatePrimaryPill {
    /// A contract state; `action` draws the fixture's `action` label (or
    /// `actionLabel`) as the primary pill, on `page` only.
    public init(
        _ state: EmptyStateCopy.State,
        actionLabel: String? = nil,
        action: (() -> Void)? = nil
    ) {
        let label = actionLabel ?? state.action
        self.size = state.size
        self.icon = EmptyStateIcon.glyph(state.icon)
        self.title = state.title
        self.description = state.description
        self.actions = EmptyStatePrimaryPill(label: label ?? "", action: action ?? {})
        self.hasActions = label != nil && action != nil
    }

    /// Inline copy another contract already locks (search, pickers, the
    /// sync debug lists, the templated offline host line). `icon` = a
    /// registry glyph (`AppIcons.navSearch`), nil = a text-only line.
    public init(
        title: String,
        description: String? = nil,
        icon: String? = nil,
        size: EmptyStateCopy.Size = .page,
        actionLabel: String? = nil,
        action: (() -> Void)? = nil
    ) {
        self.size = size
        self.icon = icon
        self.title = title
        self.description = description
        self.actions = EmptyStatePrimaryPill(label: actionLabel ?? "", action: action ?? {})
        self.hasActions = actionLabel != nil && action != nil
    }
}

/// The page state's one primary button.
public struct EmptyStatePrimaryPill: View {
    let label: String
    let action: () -> Void

    public var body: some View {
        GlassPill(label, icon: AppIcons.uiAdd, size: .md, mode: .action(action), primary: true)
    }
}

/// Concept id (`nav-inbox`) → the generated `AppIcons` glyph. The generated
/// file has no by-concept lookup, so the empty-state concepts are listed
/// here; `ExpUITests` walks `EmptyStateCopy.all` so a new fixture concept
/// fails there instead of drawing nothing.
public enum EmptyStateIcon {
    public static func glyph(_ concept: String) -> String? {
        switch concept {
        case "action-default": AppIcons.actionDefault
        case "action-run": AppIcons.actionRun
        case "auth-passkey": AppIcons.authPasskey
        case "coding-assistant": AppIcons.codingAssistant
        case "coding-diff": AppIcons.codingDiff
        case "coding-ended": AppIcons.codingEnded
        case "coding-running": AppIcons.codingRunning
        case "nav-actions": AppIcons.navActions
        case "nav-boards": AppIcons.navBoards
        case "nav-devices": AppIcons.navDevices
        case "nav-drafts": AppIcons.navDrafts
        case "nav-inbox": AppIcons.navInbox
        case "nav-issues": AppIcons.navIssues
        case "nav-my-issues": AppIcons.navMyIssues
        case "nav-threads": AppIcons.navThreads
        case "settings-boards": AppIcons.settingsBoards
        case "settings-labels": AppIcons.settingsLabels
        case "settings-mcp": AppIcons.settingsMcp
        case "settings-statuses": AppIcons.settingsStatuses
        case "settings-widget": AppIcons.settingsWidget
        case "trigger-event": AppIcons.triggerEvent
        case "ui-archive": AppIcons.uiArchive
        case "ui-attach": AppIcons.uiAttach
        case "ui-clock": AppIcons.uiClock
        case "ui-device": AppIcons.uiDevice
        case "ui-empty-search": AppIcons.uiEmptySearch
        case "ui-issue": AppIcons.uiIssue
        case "ui-team": AppIcons.uiTeam
        case "work-faces": AppIcons.workFaces
        case "work-results": AppIcons.workResults
        default: nil
        }
    }
}
