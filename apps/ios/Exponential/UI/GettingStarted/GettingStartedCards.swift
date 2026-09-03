import ExpUI
import ExpCore
import SwiftUI

/// The getting-started checklist, mounted under an empty board or an empty
/// team (EXP-698 r5).
///
/// It is the same block web renders in its sidebar and the IDE renders on its
/// welcome page: a section header with the `{done}/{total}` readout, then one
/// glass card per step carrying a state glyph, the step's own icon, its title,
/// a one-line description and the action pill that starts it. The copy lives
/// in `GettingStartedCopy` and is byte-identical on all four clients.
///
/// An empty board is exactly where a new team lands first, and it used to say
/// nothing but "No issues yet" — the checklist turns that dead end into the
/// list of things worth doing next, and disappears on its own once they are
/// all done.
struct GettingStartedCards: View {
    let progress: GettingStartedProgress
    let accountId: String
    let teamId: String
    /// Opens the create-board sheet on the host (it owns the team resolve and
    /// the sheet's `onCreated` landing).
    let onCreateBoard: () -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL

    /// Flips for a moment after the server step copies its install command —
    /// the pasteboard is silent otherwise.
    @State private var copiedInstall = false

    var body: some View {
        if !progress.entries.isEmpty && !progress.complete {
            VStack(alignment: .leading, spacing: 8) {
                GlassSectionHeader(GettingStartedCopy.sectionTitle) {
                    Text(GettingStartedCopy.progress(done: progress.done, total: progress.total))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }

                ForEach(Array(progress.entries.enumerated()), id: \.element.id) { index, entry in
                    card(entry, stepNumber: index + 1)
                }
            }
            .accessibilityIdentifier("getting-started")
        }
    }

    @ViewBuilder
    private func card(_ entry: GettingStartedProgress.Entry, stepNumber: Int) -> some View {
        // While the signals are still loading, nothing is done or locked yet:
        // every card reads as a plain numbered step.
        let state: GettingStartedProgress.EntryState = progress.loading ? .available : entry.state
        let locked = state == .locked

        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                stateGlyph(state: state, stepNumber: stepNumber)

                AppIcon(entryIcon(entry.key), size: AppIcon.Size.medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                Text(entry.key.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)

                Spacer(minLength: 8)
            }

            // Two lines, and the CARD grows to hold them (Android's
            // `maxLines = 2`, web wraps freely). At one line every description
            // and every locked hint ended in an ellipsis a third of the way
            // through its sentence — the shot showed "Runs coding sessions on
            // your machine and registers it as one of y…", which tells a new
            // user nothing. `fixedSize` is what stops the VStack from
            // compressing the label back to one line inside a fixed-height
            // card.
            Text(locked ? lockedHint(entry) : entry.key.entryDescription)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)

            if state != .done {
                GlassPill(
                    actionLabel(entry.key),
                    size: .sm,
                    mode: .action { perform(entry.key) },
                    enabled: !locked
                )
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
        .opacity(locked ? 0.6 : 1)
    }

    @ViewBuilder
    private func stateGlyph(
        state: GettingStartedProgress.EntryState,
        stepNumber: Int
    ) -> some View {
        switch state {
        case .done:
            AppIcon(AppIcons.uiSelected, size: AppIcon.Size.medium)
                .foregroundStyle(DesignTokens.Semantic.green)
                .frame(width: 22, height: 22)
        case .locked:
            AppIcon(AppIcons.uiPrivate, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(width: 22, height: 22)
        case .available:
            Text("\(stepNumber)")
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(width: 22, height: 22)
                .overlay(
                    Circle().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                )
        }
    }

    /// The action pill's label — the copy constant, except while the server
    /// step is flashing its copy confirmation.
    private func actionLabel(_ key: GettingStartedEntryKey) -> String {
        if key == .server && copiedInstall { return "Copied" }
        return key.actionLabel
    }

    /// EXP-548 concepts, not glyphs: the same registry names web and the IDE
    /// resolve, so the four checklists render identical icons.
    private func entryIcon(_ key: GettingStartedEntryKey) -> String {
        switch key {
        case .desktop: AppIcons.uiDevice
        case .github: AppIcons.uiGithub
        case .invite: AppIcons.uiInvite
        case .board: AppIcons.navBoards
        case .coding: AppIcons.navTerminal
        case .action: AppIcons.actionCreate
        case .server: AppIcons.uiServer
        }
    }

    private func lockedHint(_ entry: GettingStartedProgress.Entry) -> String {
        guard let lockedBy = entry.lockedBy else { return entry.key.entryDescription }
        return gettingStartedLockedHint(entry: entry.key, lockedBy: lockedBy)
    }

    private func perform(_ key: GettingStartedEntryKey) {
        switch key {
        case .desktop:
            openURL(AppConstants.desktopReleasesUrl)
        case .github, .invite:
            // Both live in team settings: repositories connect the GitHub App,
            // members mint the invite link.
            pushRoute(.teamSettings(accountId: accountId, teamId: teamId))
        case .board:
            onCreateBoard()
        case .coding:
            pushRoute(.agents)
        case .action:
            pushRoute(.actions)
        case .server:
            copyInstallSnippet()
        }
    }

    private func copyInstallSnippet() {
        let instanceUrl = deps.auth.accounts.first { $0.id == accountId }?.instanceUrl
        guard let origin = WebLinks.normalizedBase(instanceUrl) else { return }
        Platform.copyToPasteboard(AppConstants.serverInstallSnippet(origin: origin))
        copiedInstall = true
        Task {
            try? await Task.sleep(for: .seconds(2))
            copiedInstall = false
        }
    }
}

// MARK: - Environment

/// What a screen needs to render the checklist: the live progress, the team it
/// is scoped to, and the host's create-board entry point (the board step opens
/// the very same `CreateBoardSheet` the empty-team state does).
///
/// `IssuesHomeView` owns it and publishes it into the environment, so the
/// Issues ROOT list renders the checklist under its empty state while a pushed
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
    /// nil on every surface that does not host the checklist.
    var gettingStarted: GettingStartedContext? {
        get { self[GettingStartedContextKey.self] }
        set { self[GettingStartedContextKey.self] = newValue }
    }
}
