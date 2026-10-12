import ExpUI
import ExpCore
import SwiftUI

/// EXP-1270 (EXP-1259 D6, `getting-started.json`): Getting started ×4 = ONE
/// block in the list language: a group band (the title, `N of M`, the meter,
/// Docs as an icon button) over flat hairline-divided ROWS in the fixture's
/// order. A done row is muted with a green check and stays tappable; the ONE
/// next row is expanded (the glass-active fill, `why` and ONE primary `cta`
/// button); later rows are plain with a quiet chevron. No numbers, no locked
/// state, no descriptions on the others (web `@exp/ui` `GettingStartedList`,
/// desktop `getting_started.rs`, Android `GettingStartedList.kt`).
///
/// Shown on an EMPTY board (under "No issues yet"), on the Agent home while
/// the team has NO board, and in the bottom sheet behind Settings' "Getting
/// started" row (`GettingStartedSheet`). It hides while the signals load and
/// once every step is done.
struct GettingStartedList: View {
    let progress: GettingStartedProgress
    let accountId: String
    let teamId: String
    /// Opens the host's create-board sheet; nil = this block presents
    /// `CreateBoardSheet` itself.
    var onCreateBoard: (() -> Void)?
    /// Runs "Start a session" on a host that IS the Agent page (focus its
    /// composer); nil = push the Agent page.
    var onStartRun: (() -> Void)?
    /// The phone sheet: fill its width instead of stopping at `maxWidth`.
    var fullWidth = false

    @Environment(AppDependencies.self) private var deps
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL

    @State private var showAddDevice = false
    @State private var showInvite = false
    @State private var showCreateBoard = false

    var body: some View {
        if !progress.loading, !progress.complete, let state = progress.state, !state.entries.isEmpty {
            GettingStartedListFace(state: state, fullWidth: fullWidth, onSelect: run)
                .sheet(isPresented: $showAddDevice) {
                    AddDeviceSheet(accountId: accountId)
                }
                .sheet(isPresented: $showInvite) {
                    GlassSheetChrome(title: GettingStarted.title(.invite)) {
                        InviteLinkCreator(accountId: accountId, teamId: teamId)
                            .padding(16)
                    }
                    .accessibilityIdentifier("getting-started-invite-sheet")
                }
                .sheet(isPresented: $showCreateBoard) {
                    CreateBoardSheet(accountId: accountId, teamId: teamId)
                }
        }
    }

    /// A row's target (the model already turned a device-needing step
    /// without a device into `add-device`).
    private func run(_ entry: GettingStarted.Entry) {
        switch entry.target {
        case .createBoard:
            if let onCreateBoard { onCreateBoard() } else { showCreateBoard = true }
        case .github:
            // Phones open the ONE guided web page (SLOP-7).
            if let url = githubPageUrl { openURL(url) }
        case .addDevice, .addServer:
            // ONE device-setup block: both cards (the desktop app and the
            // server one-liner) are always there.
            showAddDevice = true
        case .agent:
            if let onStartRun {
                onStartRun()
            } else {
                pushRoute(.agent(accountId: accountId, seed: .empty))
            }
        case .invite:
            showInvite = true
        case .mcp, .widget:
            // Web + IDE only (`phones: false`): never listed here.
            break
        }
    }

    private var githubPageUrl: URL? {
        guard let instance = deps.auth.accounts.first(where: { $0.id == accountId })?.instanceUrl else {
            return nil
        }
        let base = instance.hasSuffix("/") ? String(instance.dropLast()) : instance
        return URL(string: "\(base)/integrations/github")
    }
}

/// The block's face: the band and the rows, drawn from a derived state.
/// Pure presentation, so a styleguide surface can draw the same one.
struct GettingStartedListFace: View {
    let state: GettingStarted.State
    var fullWidth = false
    let onSelect: (GettingStarted.Entry) -> Void

    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            band
            ForEach(Array(state.entries.enumerated()), id: \.element.id) { index, entry in
                // Hairlines between plain rows; the expanded row draws its
                // own top and bottom, so no divider doubles it.
                if index > 0, !entry.next, !state.entries[index - 1].next {
                    GlassDivider()
                }
                GettingStartedRow(entry: entry) { onSelect(entry) }
            }
        }
        .frame(maxWidth: fullWidth ? .infinity : CGFloat(GettingStarted.maxWidth), alignment: .leading)
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(GettingStarted.TestId.block)
    }

    /// The group band: title, `N of M`, the meter, Docs as an icon button.
    private var band: some View {
        GlassSectionBand(GettingStarted.title) {
            HStack(spacing: 12) {
                Text(state.progress)
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize()
                meter
                Spacer(minLength: 0)
                CircleIconButton(
                    AppIcons.uiDocs,
                    accessibilityLabel: GettingStarted.docsLabel
                ) { openURL(GettingStarted.docsUrl) }
                .accessibilityIdentifier(GettingStarted.TestId.docs)
            }
            .padding(.leading, 6)
        }
    }

    /// The progress meter: `meterWidth` wide at most, shrinking first.
    private var meter: some View {
        let fraction = state.total > 0 ? Double(state.done) / Double(state.total) : 0
        return GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(GlassTokens.fillActive)
                Capsule()
                    .fill(.white.opacity(TextOpacity.secondary))
                    .frame(width: proxy.size.width * fraction)
            }
        }
        .frame(maxWidth: CGFloat(GettingStarted.meterWidth))
        .frame(height: 4)
        .accessibilityHidden(true)
    }
}

/// One step's row: done (muted + check), the expanded next one, or plain
/// with a quiet chevron. Every row runs the step's target.
struct GettingStartedRow: View {
    let entry: GettingStarted.Entry
    let action: () -> Void

    private static let iconSize = CGFloat(GettingStarted.Row.iconSize)

    var body: some View {
        if entry.next {
            expanded
        } else {
            Button(action: action) {
                HStack(spacing: 12) {
                    AppIcon(Self.icon(entry.key), size: Self.iconSize)
                        .foregroundStyle(.white.opacity(entry.done ? TextOpacity.tertiary : TextOpacity.secondary))
                    Text(GettingStarted.title(entry.key))
                        .font(.body)
                        .foregroundStyle(.white.opacity(entry.done ? TextOpacity.tertiary : TextOpacity.primary))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    if entry.done {
                        AppIcon(AppIcons.uiCheck, size: Self.iconSize)
                            .foregroundStyle(DesignTokens.Semantic.green)
                            .accessibilityLabel("Done")
                    } else {
                        // The quiet chevron: muted at 45%.
                        AppIcon(AppIcons.uiChevronRight, size: 14)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary * 0.45))
                    }
                }
                .padding(.horizontal, CGFloat(GettingStarted.Row.padding))
                .frame(minHeight: CGFloat(GettingStarted.Row.heightPhone))
                .contentShape(Rectangle())
                .flatRow()
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier(GettingStarted.TestId.row(entry.key))
        }
    }

    /// The ONE next row: the glass-active fill between two hairlines, the
    /// title (medium) with `Next` trailing, the step's `why` and ONE primary
    /// button. The row and the button both run the target.
    private var expanded: some View {
        HStack(alignment: .top, spacing: 12) {
            AppIcon(Self.icon(entry.key), size: Self.iconSize)
                .foregroundStyle(.white)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(GettingStarted.title(entry.key))
                        .font(.body.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    GlassPill(GettingStarted.nextLabel)
                }
                Text(GettingStarted.why(entry.key))
                    .font(.system(size: 13))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
                GlassPill(
                    GettingStarted.cta(entry.key),
                    size: .md,
                    mode: .action(action),
                    primary: true
                )
                .accessibilityIdentifier(GettingStarted.TestId.cta)
                .padding(.top, 8)
            }
        }
        .padding(.horizontal, CGFloat(GettingStarted.Row.padding))
        .padding(.vertical, CGFloat(GettingStarted.Row.expandedPadding))
        .frame(minHeight: CGFloat(GettingStarted.Row.heightPhone), alignment: .topLeading)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GlassTokens.fillActive)
        .overlay(alignment: .top) { GlassDivider() }
        .overlay(alignment: .bottom) { GlassDivider() }
        .contentShape(Rectangle())
        .onTapGesture(perform: action)
        .accessibilityElement(children: .contain)
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier(GettingStarted.TestId.row(entry.key))
    }

    /// The fixture's concept ids → this client's generated icon names.
    static func icon(_ key: GettingStarted.Key) -> String {
        switch key {
        case .board: AppIcons.uiFolder
        case .github: AppIcons.uiGithub
        case .device: AppIcons.uiDevice
        case .run: AppIcons.actionRun
        case .invite: AppIcons.uiInvite
        case .server: AppIcons.uiServer
        case .mcp: AppIcons.uiMcp
        case .widget: AppIcons.uiWidget
        }
    }
}

/// Settings' "Getting started" row (the phones' entry, web's sidebar entry):
/// the same block in a bottom sheet, bound to the active team.
struct GettingStartedSheet: View {
    let progress: GettingStartedProgress
    let accountId: String
    let teamId: String

    var body: some View {
        GlassSheetChrome(title: nil) {
            // Full width: the rows run edge to edge like the sheet's other
            // lists (getting-started.json, Android's sheet).
            GettingStartedList(progress: progress, accountId: accountId, teamId: teamId, fullWidth: true)
                .padding(.top, GlassSheetTokens.headerTopPadding)
                .padding(.bottom, 24)
        }
        .accessibilityIdentifier("getting-started-sheet")
    }
}

// MARK: - Environment

/// What a screen needs to render the block: the live progress, the team it
/// is scoped to, and the host's create-board entry point (the board step opens
/// the very same `CreateBoardSheet` the empty-team state does).
///
/// `IssuesHomeView` owns it and publishes it into the environment, so the
/// Issues ROOT list renders the block under its empty state while a pushed
/// `.board` — a second board on a team that already has one — does not.
/// Like `PushRouteAction`, it owns its progress model and takes its handler
/// after construction, so the host holds ONE instance in `@State` and hands
/// the same object to the environment on every body pass — a fresh box per
/// pass would invalidate every reader of `\.gettingStarted` on every render.
@MainActor
final class GettingStartedContext {
    let progress = GettingStartedProgress()
    /// Opens the host's `CreateBoardSheet`; a no-op until the host wires it.
    var onCreateBoard: () -> Void = {}

    init() {}
}

private struct GettingStartedContextKey: EnvironmentKey {
    static let defaultValue: GettingStartedContext? = nil
}

extension EnvironmentValues {
    /// nil on every surface that does not host the block.
    var gettingStarted: GettingStartedContext? {
        get { self[GettingStartedContextKey.self] }
        set { self[GettingStartedContextKey.self] = newValue }
    }
}
