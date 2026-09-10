import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the team's AGENT page — the ONE launcher on every client. The
/// composer card (`AgentComposerCard`: subject chips, the message, images,
/// the labelled submit), the `@`/`#`/`:` candidate menu under it, the
/// options line (`AgentOptionsRow`), the start captions, then the caller's
/// Running and Past sessions (`AgentSessionsList`, moved here from the
/// Devices tab, which keeps machines only — web parity, EXP-818).
///
/// A PUSHED detail (no tab bar, native back), reached from the Chat FAB on
/// Devices/Actions with an empty seed and from every play button with a
/// preselection (`AgentComposerSeed`): the issue detail's Start coding, the
/// bulk bar, an action's Run, New action / a suggestion, a machine's play
/// glyph, the Fix conflicts pills. A start is only a COMMAND — the shared
/// watcher waits for the desktop's synced row and this page pushes the live
/// session once (EXP-536).
struct AgentPageView: View {
    let seed: AgentComposerSeed

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @State private var sessions: AgentsViewModel?
    @State private var composer: AgentComposerModel?
    /// nil until the relay config resolves — the composer waits for it
    /// rather than flashing the relay-off note.
    @State private var steerEnabled: Bool?
    @State private var canEditActions = false
    /// The started run's push target, consumed once.
    @State private var sessionTarget: StartedRunWatcher.StartedSession?
    /// The seed's team has been made the active one (once per push).
    @State private var alignedSeedTeam = false

    var body: some View {
        ZStack {
            AppBackground()

            if let sessions, let composer {
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        if steerEnabled == false {
                            relayOffNote
                        } else if steerEnabled == true {
                            AgentComposerCard(model: composer) { issueId in
                                deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                            }
                            // EXP-802: the same `@`/`#`/`:` menu the comment
                            // composer mounts, under the card — picks route
                            // through the draft editor, which keeps first
                            // responder, so the keyboard never drops.
                            if composer.draftEditor.showsAutocompleteMenu {
                                EditorAutocompleteMenu(model: composer.draftEditor)
                            }
                            AgentOptionsRow(model: composer)
                            captions(composer)
                        }

                        AgentSessionsList(
                            vm: sessions,
                            steerEnabled: steerEnabled == true,
                            canEditActions: canEditActions,
                            onFixConflicts: { issueId in
                                // The composer IS the launcher: seed the
                                // builtin with this row's PR in place.
                                composer.apply(AgentComposerSeed(
                                    actionId: DomainContract.builtinFixConflictsId,
                                    prIssueId: issueId
                                ))
                            }
                        )
                        .padding(.top, 8)
                    }
                    .padding()
                }
                .scrollDismissesKeyboard(.interactively)
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle("Agent")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-page")
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        .onAppear {
            alignActiveTeamToSeed()
            ensureModels()
            sessions?.activeTeamId = teamState.activeTeam?.id
            refreshCanEditActions()
            // Re-arm on every appear: pushing the session stops the
            // observation (onDisappear), popping back must resume it.
            sessions?.startObserving()
        }
        .onChange(of: teamState.activeTeam?.id) { _, teamId in
            sessions?.activeTeamId = teamId
            refreshCanEditActions()
            // The composer is bound to ONE team (its pools, its builtins'
            // teamId) — a team switch under the page starts a fresh one. A
            // composer nobody touched yet keeps the play button's seed (the
            // team resolving AFTER onAppear on a cold launch must not drop
            // the preselection); a touched one starts empty.
            if let sessions, let composer, composer.teamId != teamId {
                self.composer = makeComposer(
                    sessions: sessions, seed: composer.isPristine ? seed : .empty
                )
            }
        }
        .onDisappear {
            sessions?.stopObserving()
            composer?.startWatcher.stop()
        }
        // The resolved machine can change underneath the options (a
        // heartbeat dropping it, the pool arriving): reseed off the new one.
        .onChange(of: composer?.device?.deviceId) { _, _ in
            composer?.reconcileDevice()
        }
        // The desktop picked the start up — push the live steer screen ONCE
        // (the same destination the .agentSession route arm builds).
        .onChange(of: composer?.startWatcher.startedSession) { _, started in
            if let started {
                composer?.startWatcher.startedSession = nil
                sessionTarget = started
            }
        }
        .navigationDestination(item: $sessionTarget) { target in
            AgentSessionRouteView(sessionId: target.sessionId)
                .environment(\.accountId, accountId)
        }
    }

    /// EXP-825: the subject may sit on a NON-active team (an issue opened
    /// from the Inbox, Reviews or Search) — point the active team at it
    /// before the models build, the way the web play button routes to that
    /// team's `/t/$teamSlug/agent`. `activeTeam` resolves once the team row
    /// has synced (cold launch: the composer rebuilds on that change).
    private func alignActiveTeamToSeed() {
        guard !alignedSeedTeam else { return }
        alignedSeedTeam = true
        if let teamId = seed.teamId, teamId != teamState.activeTeamId {
            teamState.activeTeamId = teamId
        }
    }

    private func ensureModels() {
        if sessions == nil {
            sessions = AgentsViewModel(
                accountId: accountId, userId: deps.auth.userId, db: deps.db
            )
        }
        if composer == nil, let sessions {
            composer = makeComposer(sessions: sessions, seed: seed)
        }
    }

    private func makeComposer(sessions: AgentsViewModel, seed: AgentComposerSeed) -> AgentComposerModel {
        let model = AgentComposerModel(
            teamId: teamState.activeTeam?.id,
            accountId: accountId,
            deps: deps,
            sessions: sessions,
            seed: seed
        )
        Task { await model.load() }
        return model
    }

    /// Re-resolves the owner mirror for the team on screen.
    private func refreshCanEditActions() {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else {
            canEditActions = false
            return
        }
        canEditActions = TeamPermissions.resolve(
            team: teamState.activeTeam,
            currentUserId: deps.auth.userId,
            isAdmin: deps.auth.isAdmin,
            dbPool: pool
        ).isOwner
    }

    // MARK: - Captions

    /// What keeps the composer from starting (no machine, an agent the
    /// machine cannot drive, a multi-repo batch…), the "start sent" caption
    /// while the desktop picks the run up, and a refused send.
    @ViewBuilder
    private func captions(_ composer: AgentComposerModel) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            if let blocker = composer.blocker {
                Text(blocker)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityIdentifier("launch-not-ready-note")
            } else if composer.costWarning, !composer.overCap {
                Text("Large batches are token-expensive.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            if let sentCaption = composer.startWatcher.sentCaption {
                HStack(spacing: 6) {
                    ProgressView().controlSize(.small).tint(.white)
                    Text(sentCaption)
                        .font(.caption2)
                }
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            if let failure = composer.startWatcher.failure {
                Text(failure)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        .padding(.horizontal, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Web parity: without the relay nothing here can be started — the page
    /// still lists the sessions that synced in.
    private var relayOffNote: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.small)
            Text("Remote start isn't available on this server. Start runs from the desktop app; live sessions show up below.")
                .font(.caption)
            Spacer(minLength: 0)
        }
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
    }
}
