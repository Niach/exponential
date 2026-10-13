import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the team's AGENT page — the ONE launcher on every client.
/// EXP-1253 (EXP-1259 D1): it is HOME (composer-home.json): the headline (the
/// disc mark + the time-of-day greeting, `HomeGreeting`, computed once per
/// mount) while nothing is picked, else the subject headline (EXP-1038,
/// `AgentComposerHeadline`: the contract verb plus the action or issue
/// chips); the composer card (`AgentComposerCard`: placeholder "What do you
/// want to build?", the starting row after Start); the `@`/`:` candidate
/// menu under it (`#` opens THE search list in a sheet, SLOP-28); the options
/// line (`AgentOptionsRow`); then Getting started while the team has no
/// board, and (EXP-1284) the caller's Running THREADS (`AgentThreadLists`:
/// a live session of mine, every team, banded, plus my live sessions no
/// thread names yet; a row opens the thread). No Recent band and no
/// suggestion rows (Danny 2026-10-12): the toolbar's history sheet (threads,
/// banded per team) is the one place for recent work. The brand-mark
/// watermark is gone ×4.
///
/// Two shapes: the Agent ROOT (`isTabRoot`, an empty seed as the stack's
/// root — EXP-1210: a tab root like its siblings, no back — the app's
/// landing screen, under the floating tab bar, its chat arm selected), and a
/// PUSHED detail (no tab bar, native back) from every play button with a
/// preselection (`AgentComposerSeed`): the issue detail's Start coding, the
/// bulk bar, an action's Run, New action / a suggestion, a machine's play
/// glyph, the Fix conflicts pills. A start is only a COMMAND — the shared
/// watcher waits for the desktop's synced row and this page pushes the live
/// session once (EXP-536).
struct AgentPageView: View {
    let seed: AgentComposerSeed
    /// The Agent tab root: the floating tab bar sits over it, so the scroller
    /// reserves its clearance (`.tabBarBottomInset()`) — dropped while the
    /// keyboard is up, when the navigator hides the bar.
    var isTabRoot: Bool = false

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @State private var sessions: AgentsViewModel?
    /// EXP-1284: the page lists THREADS, not bare runs (Running / Recent,
    /// `partitionThreadList`).
    @State private var threads: ThreadListModel?
    @State private var composer: AgentComposerModel?
    /// nil until the relay config resolves — the composer waits for it
    /// rather than flashing the relay-off note.
    @State private var steerEnabled: Bool?
    /// The started run's push target, consumed once.
    @State private var sessionTarget: StartedRunWatcher.StartedSession?
    /// On screen (appear → disappear): only a visible page pushes a landing.
    @State private var visible = false
    /// EXP-923: the history sheet — the finished runs, off the toolbar glyph.
    @State private var showRecent = false
    /// The thread picked in that sheet, pushed once the sheet is gone (a push
    /// racing its own dismissal lands on nothing).
    @State private var pendingRecent: String?
    /// The seed's team has been made the active one (once per push).
    @State private var alignedSeedTeam = false
    /// EXP-1253: the greeting THIS mount shows — computed once, it never
    /// flips while you type (web `useState(() => homeGreeting(...))`).
    @State private var greeting: String?
    /// EXP-1270: Getting started's signals for the active team (the block
    /// follows the composer while the team has no board).
    @State private var gettingStarted = GettingStartedProgress()
    @State private var keyboardVisible = false
    /// EXP-1196: the not-ready row's Sign in pill → the remote sign-in.
    @State private var loginTarget: AgentLoginTarget?
    /// The not-ready row's Import pill (`agentImportFlow`).
    @State private var importTarget: AgentImportTarget?
    /// EXP-1196: the not-ready row's Update while `agent_update` is out.
    @State private var updatingAgent: String?
    @Environment(\.toaster) private var toaster
    @Environment(\.pushRoute) private var pushRoute

    private var reservesTabBar: Bool { isTabRoot && !keyboardVisible }

    var body: some View {
        ZStack {
            AppBackground()

            if sessions != nil, let composer {
                // EXP-862: with nothing running, the
                // composer column is CENTRED in the page (×4: web's
                // `justify-center`, the IDE's `min_h_full`) — a lone prompt box
                // pinned under the nav bar over an empty screen read as a page
                // that failed to load.
                // EXP-923: only the RUNNING rows count now — history moved
                // into its own sheet, so an empty Running band is an empty
                // page.
                let centred = (threads?.running.isEmpty ?? true) && (threads?.orphans.isEmpty ?? true)
                GeometryReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        if steerEnabled == false {
                            homeHeadline
                                .padding(.bottom, CGFloat(HomeGreeting.Geometry.headlineToCard) - 12)
                            relayOffNote
                        } else if steerEnabled == true {
                            // EXP-1038: the run's SUBJECT heads the page —
                            // the contract verb ("Run" · "Implement") with
                            // the action or issue chips beside it. It sits
                            // ABOVE everything the composer offers, so a
                            // prefilled start reads as a thing about to be
                            // sent, not as a chip. A CHAT draws no row at all
                            // (web `LaunchHeadline` returns null, ×4): the
                            // field's placeholder already says "Ask the
                            // agent…", and the same sentence twice is not a
                            // heading.
                            if composer.subject != AgentComposerPrompt.Subject.none {
                                AgentComposerHeadline(model: composer)
                            } else {
                                // EXP-1253: nothing picked = the greeting leads.
                                homeHeadline
                                    .padding(.bottom, CGFloat(HomeGreeting.Geometry.headlineToCard) - 12)
                            }
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

                        homeGettingStarted

                        // EXP-1284 (draft 3; T4/T6): the Running THREADS —
                        // every team, banded; my live sessions no thread
                        // names yet as plain session rows. A row opens the
                        // thread.
                        if let threads {
                            AgentThreadLists(
                                model: threads,
                                onOpen: { threadId in
                                    pushRoute(.thread(accountId: accountId, threadId: threadId))
                                },
                                onOpenSession: { sessionId in
                                    pushRoute(.runThread(accountId: accountId, sessionId: sessionId))
                                }
                            )
                            .padding(.top, 8)
                        }
                    }
                    .padding()
                    .frame(
                        // The bar's clearance comes out of the centring
                        // height, else the centred column overflows by it.
                        minHeight: centred
                            ? proxy.size.height - (reservesTabBar ? tabBarBottomClearance : 0)
                            : nil,
                        alignment: centred ? .center : .top
                    )
                    // EXP-1276: tap-outside keyboard dismissal, the catcher
                    // IssueFaceView mounts (EXP-246) — BEHIND the content, so
                    // only dead space (gaps, padding, the empty page) takes
                    // the tap; the field, the pills and the rows keep winning
                    // hit-testing.
                    .background {
                        Color.clear
                            .contentShape(Rectangle())
                            .onTapGesture { UIApplication.endEditing() }
                    }
                }
                .scrollDismissesKeyboard(.interactively)
                // On the SCROLLER itself (EXP-36), so the composer and the
                // sessions list clear the floating bar.
                .tabBarBottomInset(reservesTabBar)
                }
            } else {
                // EXP-1275: the first load is a placeholder, not a spinner.
                SkeletonRows()
                    .padding()
                    .frame(maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationTitle("Agent")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        // EXP-923: history is a TOOLBAR glyph, not a fold under the composer
        // (the ×4 rule) — the page lists what runs, the sheet what is over.
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showRecent = true } label: {
                    AppIcon(AppIcons.settingsSessions, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .accessibilityLabel("Recent sessions")
                .accessibilityIdentifier("agent-history-button")
            }
        }
        .sheet(item: $loginTarget) { target in
            if let sessions {
                AgentLoginSheet(viewModel: sessions, target: target)
            }
        }
        .agentImportFlow($importTarget, devices: sessions?.devices ?? [])
        // T-N1: the history sheet lists THREADS (banded per team); a row
        // opens the thread once the sheet has closed.
        .sheet(isPresented: $showRecent, onDismiss: pushPendingRecent) {
            if let threads {
                RecentThreadsSheet(
                    model: threads,
                    onOpen: { threadId in
                        pendingRecent = threadId
                        showRecent = false
                    }
                )
                .environment(teamState)
            }
        }
        // EXP-1288: Recent's threads are read only while the sheet lists them.
        .onChange(of: showRecent) { _, open in
            threads?.includeRecent = open
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in
            keyboardVisible = true
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in
            keyboardVisible = false
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-page")
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        .onAppear {
            visible = true
            // A session that landed (unchanged draft) while the page was
            // off-screen opens on the way back, the way Android's ViewModel
            // hands it over; pushed a beat later so it never races the pop.
            if let landed = composer?.landedSession {
                composer?.landedSession = nil
                Task { @MainActor in land(landed) }
            }
            if greeting == nil {
                greeting = HomeGreeting.greeting(name: deps.auth.userName)
            }
            alignActiveTeamToSeed()
            ensureModels()
            // A composer kept through a team change (its draft or start)
            // follows the team once there is nothing left to lose.
            rebindComposer(to: teamState.activeTeam?.id)
            sessions?.activeTeamId = teamState.activeTeam?.id
            threads?.start()
            // Re-arm on every appear: pushing the session stops the
            // observation (onDisappear), popping back must resume it.
            sessions?.startObserving()
        }
        // EXP-1270: Getting started's signals follow the active team.
        .task(id: "\(accountId)/\(teamState.activeTeam?.id ?? "")") {
            if let teamId = teamState.activeTeam?.id {
                gettingStarted.bind(accountId: accountId, teamId: teamId, deps: deps)
            } else {
                gettingStarted.stop()
            }
        }
        .onChange(of: teamState.activeTeam?.id) { _, teamId in
            sessions?.activeTeamId = teamId
            rebindComposer(to: teamId)
        }
        // A start that was in flight through a team change (landed or timed
        // out) no longer holds the composer: it follows the team now.
        .onChange(of: composer?.isStartInFlight) { _, inFlight in
            if inFlight == false { rebindComposer(to: teamState.activeTeam?.id) }
        }
        .onDisappear {
            visible = false
            sessions?.stopObserving()
            threads?.stop()
            // B5: the start watch is NOT stopped here: it lives on the
            // composer model, bounded by its own window, so a session that
            // lands while the page is off-screen still clears the error and
            // the unchanged draft.
        }
        // The resolved machine can change underneath the options (a
        // heartbeat dropping it, the pool arriving): reseed off the new one.
        .onChange(of: composer?.device?.deviceId) { _, _ in
            composer?.reconcileDevice()
        }
        // The desktop picked the start up — push the live steer screen ONCE
        // (the same destination the .agentSession route arm builds). The
        // model already ended the starting state and cleared the draft; it
        // sets this only when the draft was unchanged since the send. Off
        // screen it waits for the page to come back (`onAppear`).
        .onChange(of: composer?.landedSession) { _, started in
            if let started, visible {
                composer?.landedSession = nil
                land(started)
            }
        }
        // EXP-1284: the started run opens as its THREAD once the run's item
        // synced (the New run becomes the thread), else its Work screen.
        .navigationDestination(item: $sessionTarget) { target in
            RunThreadScreen(sessionId: target.sessionId)
                .environment(\.accountId, accountId)
        }
        // EXP-897/EXP-980/SLOP-3: starting on BLOCKED work asks first: an
        // ordinary run, a stacked PR, or nothing. EXP-1215: the app's alert
        // card, the transitive chain in its content slot.
        .glassAlert(
            item: Binding(
                get: { composer?.blockedPrompt },
                set: { if $0 == nil { composer?.blockedPrompt = nil } }
            )
        ) { prompt in
            BlockedStartAlert.card(
                prompt,
                onStartAnyway: { composer?.startAnyway() },
                onStartStacked: { composer?.startStacked(prompt) },
                onOpenIssue: { id in
                    composer?.blockedPrompt = nil
                    deps.deepLinkBus.navigateToIssue(id, accountId: accountId)
                }
            )
        }
    }

    /// T11/T-N7/T-N9: where a landed start opens. A start INTO a thread
    /// lands back on that thread's page (the navigator pops to it when it is
    /// below, never a duplicate) on its Conversation; any other start opens
    /// the session's thread once it synced (`RunThreadScreen`, Work fallback).
    private func land(_ started: StartedRunWatcher.StartedSession) {
        if let threadId = composer?.threadId {
            pushRoute(.thread(accountId: accountId, threadId: threadId, child: .conversation))
        } else {
            sessionTarget = started
        }
    }

    /// EXP-923: the sheet's pick, pushed once it has actually closed.
    /// T-N1: a thread now.
    private func pushPendingRecent() {
        guard let threadId = pendingRecent else { return }
        pendingRecent = nil
        pushRoute(.thread(accountId: accountId, threadId: threadId))
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
        if threads == nil {
            threads = ThreadListModel(accountId: accountId, userId: deps.auth.userId, db: deps.db)
        }
    }

    /// The composer is bound to ONE team (its pools, its builtins' teamId),
    /// so a team change ALWAYS rebinds it: a PRISTINE composer keeps the
    /// play button's seed (the team resolving AFTER onAppear on a cold
    /// launch must not drop the preselection), any other one is rebuilt for
    /// the new team with its text and attachments carried over (the subject
    /// was the old team's). The ONE exception is a start in flight (the
    /// uploads, the request, the starting wait): it finishes on its team and
    /// the rebind follows as it ends (`onChange(of: isStartInFlight)`). The
    /// quiet watch after a timeout never holds the composer: it is cancelled
    /// with the old model, or a Start pressed under the new team would land
    /// on the old one.
    private func rebindComposer(to teamId: String?) {
        guard let sessions, let composer, composer.teamId != teamId,
              !composer.isStartInFlight else { return }
        composer.release()
        if composer.isPristine {
            self.composer = makeComposer(sessions: sessions, seed: seed)
        } else {
            let next = makeComposer(sessions: sessions, seed: .empty)
            next.carryDraft(from: composer)
            self.composer = next
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
        // EXP-1277: seed the options off the machine the pool has ALREADY
        // resolved. The `onChange(of: deviceId)` below only fires when that
        // id changes — a composer rebuilt for the same machine (the team
        // resolving after onAppear) would otherwise never seed.
        model.reconcileDevice()
        Task { await model.load() }
        return model
    }

    // MARK: - Home (EXP-1253)

    /// The disc mark + ONE line of Inter semibold (composer-home.json
    /// `geometry`, phones: `markPhone` / `headlinePhone`), centred.
    private var homeHeadline: some View {
        HStack(spacing: CGFloat(HomeGreeting.Geometry.gap)) {
            ExpLogoMark(size: CGFloat(HomeGreeting.Geometry.markPhone), color: .white)
                .accessibilityIdentifier(HomeGreeting.TestId.mark)
            Text(greeting ?? "")
                .font(.system(size: CGFloat(HomeGreeting.Geometry.headlinePhone), weight: .semibold))
                .tracking(-0.3)
                .foregroundStyle(.white)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier(HomeGreeting.TestId.greeting)
        }
        .frame(maxWidth: .infinity)
    }

    /// EXP-1270: while the team has NO board, Getting started follows the
    /// composer, `composerToGettingStarted` below the options line ("Start a
    /// run" focuses this page's composer).
    @ViewBuilder
    private var homeGettingStarted: some View {
        if let teamId = teamState.activeTeam?.id,
           gettingStarted.entries.contains(where: { $0.key == .board && !$0.done }) {
            GettingStartedList(
                progress: gettingStarted,
                accountId: accountId,
                teamId: teamId,
                onStartRun: { composer?.draftEditor.focusEnd() }
            )
            .padding(.top, CGFloat(HomeGreeting.Geometry.composerToGettingStarted) - 12)
        }
    }

    // MARK: - Captions

    /// What keeps the composer from starting (no machine, an agent the
    /// machine cannot drive, a multi-repo batch…). EXP-1271: the wait for the
    /// desktop is the card's starting row now, and a refused or timed-out
    /// start a toast (FEED-79).
    @ViewBuilder
    private func captions(_ composer: AgentComposerModel) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            if let row = composer.notReadyRow, let device = composer.device {
                // EXP-1196/1219: the machine's failing doctor row, same row
                // and action (Import pill included) as its readiness block.
                DeviceReadinessView(
                    row: row,
                    busy: updatingAgent == row.key,
                    onAction: { row in runNotReadyAction(row, device: device) },
                    onImport: { row in importTarget = AgentImportTarget(row: row, device: device) }
                )
                .accessibilityIdentifier("launch-not-ready-row")
            } else if let blocker = composer.blocker {
                Text(blocker)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityIdentifier("launch-not-ready-note")
            } else if composer.costWarning, !composer.overCap {
                Text("Large batches are token-expensive.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            // EXP-836: a play button named a machine this composer cannot
            // start on — say which and why (web parity), instead of quietly
            // running on the default one. Independent of `blocker`: the run
            // CAN go, just not where it was asked to.
            if let requestNote = composer.deviceRequestNote {
                Text(requestNote)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.yellow)
                    .accessibilityIdentifier("launch-device-request-note")
            }
        }
        .padding(.horizontal, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A phone is always ANOTHER device: only the remote actions reach here.
    private func runNotReadyAction(_ row: DeviceReadiness.Row, device: SteerDevice) {
        switch row.action {
        case "update":
            guard updatingAgent == nil else { return }
            updatingAgent = row.key
            let api = deps.devicesApi
            let account = accountId
            Task {
                do {
                    try await api.requestAgentUpdate(
                        accountId: account, deviceId: device.deviceId, agent: row.key
                    )
                } catch {
                    toaster.error(
                        FeedbackStates.Errors.title(verb: "update", thing: "the agent"),
                        description: error.userFacingMessage
                    )
                }
                updatingAgent = nil
            }
        case "sign_in":
            loginTarget = AgentLoginTarget(
                deviceId: device.deviceId,
                deviceLabel: LaunchVocabulary.deviceName(device),
                agent: row.key
            )
        default:
            break
        }
    }

    /// Web parity: without the relay nothing here can be started — the page
    /// still lists the sessions that synced in.
    private var relayOffNote: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.small)
            Text("Remote start isn't available on this server. Start sessions from the desktop app; live sessions show up below.")
                .font(.caption)
            Spacer(minLength: 0)
        }
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
    }
}
