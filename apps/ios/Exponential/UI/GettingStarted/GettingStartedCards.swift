import ExpUI
import ExpCore
import SwiftUI

/// EXP-1270 (EXP-1259 D6, `getting-started.json`): Getting started ×4 = ONE
/// block: a header (the title, `N of M`, the meter, the Docs button) over ONE
/// horizontal strip of square cards (`GettingStarted.Card`, 104×112) in the
/// fixture's order. A card is the step's icon top-left, a green check (done)
/// or its muted number top-right, the title bottom-left; the NEXT step is
/// highlighted; done cards stay tappable; no descriptions, no locked state,
/// no per-card buttons (web `@exp/ui` `GettingStartedStrip`, desktop
/// `getting_started.rs`, Android `GettingStartedStrip.kt`).
///
/// Shown on an EMPTY board (under "No issues yet"), on the Agent home while
/// the team has NO board, and in the bottom sheet behind Settings' "Getting
/// started" row (`GettingStartedSheet`). It hides while the signals load and
/// once every step is done.
struct GettingStartedCards: View {
    let progress: GettingStartedProgress
    let accountId: String
    let teamId: String
    /// Opens the host's create-board sheet; nil = this block presents
    /// `CreateBoardSheet` itself.
    var onCreateBoard: (() -> Void)?
    /// Runs "Start a run" on a host that IS the Agent page (focus its
    /// composer); nil = push the Agent page.
    var onStartRun: (() -> Void)?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL

    @State private var showAddDevice = false
    @State private var showInvite = false
    @State private var showCreateBoard = false

    var body: some View {
        if !progress.loading, !progress.complete, let state = progress.state, !state.entries.isEmpty {
            GettingStartedStrip(state: state, onSelect: run)
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

    /// A card's target (the model already turned a device-needing step
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

/// The block's face: the header and the strip, drawn from a derived state.
/// Pure presentation, so the styleguide screen draws the same one.
struct GettingStartedStrip: View {
    let state: GettingStarted.State
    let onSelect: (GettingStarted.Entry) -> Void

    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: CGFloat(GettingStarted.Card.gap)) {
                    ForEach(Array(state.entries.enumerated()), id: \.element.id) { index, entry in
                        GettingStartedCard(entry: entry, number: index + 1) { onSelect(entry) }
                    }
                }
                .padding(.vertical, 1)
            }
            .scrollClipDisabled()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(GettingStarted.TestId.block)
    }

    private var header: some View {
        HStack(spacing: 12) {
            Text(GettingStarted.title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .fixedSize()
            Text(state.progress)
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .fixedSize()
            meter
            Spacer(minLength: 0)
            GlassPill(
                GettingStarted.docsLabel,
                icon: AppIcons.uiDocs,
                mode: .action { openURL(GettingStarted.docsUrl) }
            )
            .accessibilityIdentifier(GettingStarted.TestId.docs)
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

/// One square card.
struct GettingStartedCard: View {
    let entry: GettingStarted.Entry
    /// The 1-based position in the visible list.
    let number: Int
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top) {
                    AppIcon(Self.icon(entry.key), size: CGFloat(GettingStarted.Card.icon))
                        .foregroundStyle(.white.opacity(entry.done ? TextOpacity.tertiary : TextOpacity.primary))
                    Spacer(minLength: 0)
                    if entry.done {
                        AppIcon(AppIcons.uiCheck, size: 16)
                            .foregroundStyle(DesignTokens.Semantic.green)
                            .accessibilityLabel("Done")
                    } else {
                        Text("\(number)")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                Spacer(minLength: 0)
                Text(GettingStarted.title(entry.key))
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(.white.opacity(entry.done ? TextOpacity.tertiary : TextOpacity.primary))
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(CGFloat(GettingStarted.Card.padding))
            .frame(
                width: CGFloat(GettingStarted.Card.width),
                height: CGFloat(GettingStarted.Card.height),
                alignment: .topLeading
            )
            .background(
                entry.next ? GlassTokens.fillActive : Color.clear,
                in: RoundedRectangle(cornerRadius: DesignTokens.Radius.lg)
            )
            .overlay(
                RoundedRectangle(cornerRadius: DesignTokens.Radius.lg)
                    .strokeBorder(
                        entry.next ? Color.white.opacity(0.3) : GlassTokens.strokeCard,
                        lineWidth: GlassTokens.hairline
                    )
            )
            .contentShape(RoundedRectangle(cornerRadius: DesignTokens.Radius.lg))
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(GettingStarted.TestId.card(entry.key))
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
            GettingStartedCards(progress: progress, accountId: accountId, teamId: teamId)
                .padding(.horizontal, 16)
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
