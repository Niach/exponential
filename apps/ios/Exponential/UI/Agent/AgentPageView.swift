import ExpCore
import ExpUI
import SwiftUI

/// EXP-825/EXP-1291: HOME — the ONE launcher on every client. Top to bottom:
/// the DAWN (FEED-100: the big brand mark rising behind the composer) while
/// nothing is picked, else the subject headline (EXP-1038,
/// `AgentComposerHeadline`: the contract verb plus the issue chips, or the
/// prompt / agent chip); the composer card (`AgentComposerCard`, placeholder "What do you
/// want to build?", the starting row after Start); the `@`/`:` candidate
/// menu under it (S-3: no options line, the "+" holds the runner). EXP-1316
/// (composer-home.json `phone`): the column is pinned to the BOTTOM (above
/// the tab bar, riding the keyboard), the dawn standing on the card; Getting
/// started (no board yet) sits above the dawn; no Running list — the Threads
/// tab lists every thread.
///
/// Two shapes: the Home ROOT (`isTabRoot`, an empty seed as the stack's
/// root — EXP-1210: a tab root like its siblings, no back — the app's
/// landing screen, under the floating tab bar, its chat arm selected), and a
/// PUSHED detail (no tab bar, native back) from every play button with a
/// preselection (`AgentComposerSeed`): the issue's Start, the bulk bar, an
/// action's Run, a machine's play glyph. A send is ONE `sessions.send` in
/// the ACTIVE team; the page opens the new thread at once.
struct AgentPageView: View {
    let seed: AgentComposerSeed
    /// The Home tab root: the floating tab bar sits over it, so the scroller
    /// reserves its clearance (`.tabBarBottomInset()`) — dropped while the
    /// keyboard is up, when the navigator hides the bar.
    var isTabRoot: Bool = false

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    /// The composer's machine pool (the Devices tab's model).
    @State private var devicesModel: AgentsViewModel?
    @State private var composer: AgentComposerModel?
    /// On screen (appear → disappear): only a visible page opens a landing.
    @State private var visible = false
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
    /// S-3 THE AGENT ROW's pick (composer-home.json `agentRow`): nil = the
    /// default (the viewer's Assistant, else Temporary session). A play
    /// button's seed (issues, a prompt, a machine) starts on Temporary
    /// session: it is a launch.
    @State private var agentPick: String?

    private var reservesTabBar: Bool { isTabRoot && !keyboardVisible }

    /// The team the composer starts in: a play button's subject team (an
    /// issue opened from the Inbox or Search may sit in another team) for
    /// THIS pushed page only, else the active team (the current board's).
    /// The app's active team never moves from here — the board and team
    /// switch together in the board switcher.
    private var composerTeamId: String? { seed.teamId ?? teamState.activeTeam?.id }

    var body: some View {
        ZStack {
            AppBackground()

            if devicesModel != nil, let composer {
                GeometryReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        homeGettingStarted
                        Spacer(minLength: 0)
                        // EXP-1038: a picked SUBJECT heads the column — the
                        // contract verb ("Run" · "Implement") with the
                        // action or issue chips beside it. Nothing picked
                        // = the dawn stands on the composer (FEED-100).
                        // The pair's own spacing: the headline keeps the
                        // column's 12pt, the dawn stands on the card
                        // (`HomeDawn.gap`).
                        // S-3: an agent pick is the agent row's, never a
                        // headline: the dawn stands on the row + card.
                        let headlined = composer.subject != AgentComposerPrompt.Subject.none
                            && composer.subject != AgentComposerPrompt.Subject.agent
                        VStack(alignment: .leading, spacing: headlined ? 12 : HomeDawn.gap) {
                            if headlined {
                                AgentComposerHeadline(model: composer)
                            } else {
                                HomeDawn()
                            }
                            VStack(alignment: .leading, spacing: CGFloat(ComposerAgentRow.rowToCard)) {
                                agentRow(composer)
                                AgentComposerCard(model: composer) { issueId in
                                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                                }
                            }
                        }
                        // EXP-802: the same `@`/`#`/`:` menu the comment
                        // composer mounts, under the card — picks route
                        // through the draft editor, which keeps first
                        // responder, so the keyboard never drops.
                        if composer.draftEditor.showsAutocompleteMenu {
                            EditorAutocompleteMenu(model: composer.draftEditor)
                        } else if slashMenuVisible(composer) {
                            // NEW-2: the `/` menu = the team's prompts (no
                            // session yet, so no builtins); a pick becomes the
                            // subject chip and clears the `/name` typed.
                            SlashCommandMenu(
                                candidates: composer.slashPrompts.map(SlashCandidate.prompt),
                                highlighted: 0
                            ) { candidate in
                                if case let .prompt(prompt) = candidate {
                                    composer.pickPrompt(prompt.id)
                                    composer.draftText = ""
                                }
                            }
                            .accessibilityIdentifier("agent-composer-slash-menu")
                        }
                        captions(composer)
                    }
                    .padding()
                    .frame(
                        // EXP-1316: pinned to the bottom — the bar's
                        // clearance comes out of the height, else the column
                        // overflows by it; the keyboard shrinks the scroller
                        // and the column rides up with it.
                        minHeight: proxy.size.height - (reservesTabBar ? tabBarBottomClearance : 0),
                        alignment: .bottom
                    )
                    // EXP-1276: tap-outside keyboard dismissal — BEHIND the
                    // content, so only dead space takes the tap.
                    .background {
                        Color.clear
                            .contentShape(Rectangle())
                            .onTapGesture { UIApplication.endEditing() }
                    }
                }
                .scrollDismissesKeyboard(.interactively)
                // On the SCROLLER itself (EXP-36), so the composer clears
                // the floating bar.
                .tabBarBottomInset(reservesTabBar)
                }
            } else {
                // EXP-1275: the first load is a placeholder, not a spinner.
                SkeletonRows()
                    .padding()
                    .frame(maxHeight: .infinity, alignment: .top)
            }
        }
        // #EXP-1384: the page header (`copy.headers.agent`).
        .pageHeader(Chats.Copy.Headers.agent)
        .sheet(item: $loginTarget) { target in
            if let devicesModel {
                AgentLoginSheet(viewModel: devicesModel, target: target)
            }
        }
        .agentImportFlow($importTarget, devices: devicesModel?.devices ?? [])
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in
            keyboardVisible = true
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in
            keyboardVisible = false
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-page")
        .onAppear {
            visible = true
            // A thread a send started while the page was off-screen opens on
            // the way back; pushed a beat later so it never races the pop.
            if let rootId = composer?.landedRootId {
                composer?.landedRootId = nil
                Task { @MainActor in open(rootId) }
            }
            if let agentId = composer?.landedAgentId {
                composer?.landedAgentId = nil
                Task { @MainActor in openAgent(agentId) }
            }
            ensureModels()
            // A composer kept through a team change (its draft or send)
            // follows the team once there is nothing left to lose.
            rebindComposer(to: composerTeamId)
            devicesModel?.activeTeamId = composerTeamId
            // Re-arm on every appear: a push stops the observation
            // (onDisappear), popping back must resume it.
            devicesModel?.startObserving()
        }
        // EXP-1270: Getting started's signals follow the active team.
        .task(id: "\(accountId)/\(teamState.activeTeam?.id ?? "")") {
            if let teamId = teamState.activeTeam?.id {
                gettingStarted.bind(accountId: accountId, teamId: teamId, deps: deps)
            } else {
                gettingStarted.stop()
            }
        }
        .onChange(of: composerTeamId) { _, teamId in
            devicesModel?.activeTeamId = teamId
            rebindComposer(to: teamId)
        }
        // A send that was in flight through a team change no longer holds
        // the composer once it answers: it follows the team now.
        .onChange(of: composer?.isStartInFlight) { _, inFlight in
            if inFlight == false {
                rebindComposer(to: composerTeamId)
                // The send cleared the model's agent; the row keeps its pick.
                syncAgentPick()
            }
        }
        .onDisappear {
            visible = false
            devicesModel?.stopObserving()
        }
        // S-3: the viewer's Assistant exists once the page mounts (once per
        // team per app run).
        .task(id: "\(accountId)/\(composerTeamId ?? "")") {
            if let teamId = composerTeamId {
                await AssistantEnsure.run(deps, accountId: accountId, teamId: teamId)
            }
        }
        // S-3: the agent row's pick drives the composer's agent; the agents
        // pool arriving (the Assistant created) re-resolves the default.
        .onChange(of: agentPick) { _, _ in syncAgentPick() }
        .onChange(of: composer?.agents.map(\.id)) { _, _ in syncAgentPick() }
        .onChange(of: composer.map(ObjectIdentifier.init)) { _, _ in syncAgentPick() }
        // A send (or a rebuilt composer) clears the model's agent: the row
        // keeps its pick, so put it back; an issue or prompt subject picked
        // in the + menu is a launch: the row follows to Temporary session.
        .onChange(of: composer?.subject) { _, subject in
            let next = ComposerAgentRow.pickForSubject(subject, current: agentPick)
            if next != agentPick { agentPick = next } else { syncAgentPick() }
        }
        // The resolved machine can change underneath the options (a
        // heartbeat dropping it, the pool arriving): reseed off the new one.
        .onChange(of: composer?.device?.deviceId) { _, _ in
            composer?.reconcileDevice()
        }
        // `sessions.send` answered with the new root: open its thread ONCE.
        // Off screen it waits for the page to come back (`onAppear`).
        .onChange(of: composer?.landedRootId) { _, rootId in
            if let rootId, visible {
                composer?.landedRootId = nil
                open(rootId)
            }
        }
        // NEW-3: a message into an agent's DM opens that DM.
        .onChange(of: composer?.landedAgentId) { _, agentId in
            if let agentId, visible {
                composer?.landedAgentId = nil
                openAgent(agentId)
            }
        }
    }

    // MARK: - Agent row (S-3)

    private func agentRowState(_ composer: AgentComposerModel) -> ComposerAgentRow.State<AgentDefinitionEntity> {
        ComposerAgentRow.composerAgentRow(agents: composer.agents, me: deps.auth.userId, picked: agentPick)
    }

    /// THE agent row over the card: the Assistant (brand mark) first, the
    /// team agents (icon in colour), the dotted Temporary session circle.
    private func agentRow(_ composer: AgentComposerModel) -> some View {
        let state = agentRowState(composer)
        let items = state.agents.map { agent in
            ComposerAgentRowView.Item(
                id: agent.id,
                name: agent.name,
                glyph: agent.isAssistant ? .brand : .icon(
                    BoardTypeDisplay.iconName(for: agent.icon) ?? SessionTree.ListCopy.agentDefaultIcon,
                    Color(hex: agent.color) ?? Color(hex: SessionTree.ListCopy.agentDefaultColor) ?? .white
                )
            )
        } + [.temporary]
        return ComposerAgentRowView(label: state.label, items: items, selected: state.selected) { id in
            agentPick = id
        }
    }

    /// The picked agent = the composer's agent subject (the + menu hides its
    /// runner and work rows, the send goes into that agent's conversation);
    /// Temporary session = today's launch.
    private func syncAgentPick() {
        guard let composer, !composer.isStartInFlight else { return }
        let want = agentRowState(composer).agent?.id
        guard composer.agentId != want else { return }
        if let want {
            composer.pickAgent(want)
        } else {
            // #EXP-1450: an agent over issue chips reads the issues subject,
            // so clear on the recipient itself, never on the subject.
            composer.clearAgent()
        }
    }

    /// A started thread opens on its Conversation.
    private func open(_ rootId: String) {
        pushRoute(.thread(accountId: accountId, threadId: rootId))
    }

    /// NEW-3: the caller's DM with an agent.
    private func openAgent(_ agentId: String) {
        pushRoute(.agentChannel(accountId: accountId, agentId: agentId))
    }

    /// NEW-2: the `/` menu is up while the field has focus and the draft is a
    /// `/name` token matching a prompt.
    private func slashMenuVisible(_ composer: AgentComposerModel) -> Bool {
        composer.draftEditor.isEditing && !composer.slashPrompts.isEmpty
    }

    private func ensureModels() {
        if devicesModel == nil {
            devicesModel = AgentsViewModel(
                accountId: accountId, userId: deps.auth.userId, db: deps.db
            )
        }
        if composer == nil, let devicesModel {
            composer = makeComposer(devicesModel: devicesModel, seed: seed)
            // A play button's subject is a launch: Temporary session.
            if agentPick == nil {
                agentPick = ComposerAgentRow.seedPick(
                    issueIds: seed.effectiveIssueIds, promptId: seed.promptId, deviceId: seed.deviceId
                )
            }
        }
    }

    /// The composer is bound to ONE team (its pools, the send's `teamId`),
    /// so a team change ALWAYS rebinds it: a PRISTINE composer keeps the
    /// play button's seed (the team resolving AFTER onAppear on a cold
    /// launch must not drop the preselection), any other one is rebuilt for
    /// the new team with its text and attachments carried over (the subject
    /// was the old team's). The ONE exception is a send in flight: it
    /// finishes on its team and the rebind follows as it ends.
    private func rebindComposer(to teamId: String?) {
        guard let devicesModel, let composer, composer.teamId != teamId,
              !composer.isStartInFlight else { return }
        composer.release()
        if composer.isPristine {
            self.composer = makeComposer(devicesModel: devicesModel, seed: seed)
        } else {
            let next = makeComposer(devicesModel: devicesModel, seed: .empty)
            next.carryDraft(from: composer)
            self.composer = next
        }
    }

    private func makeComposer(devicesModel: AgentsViewModel, seed: AgentComposerSeed) -> AgentComposerModel {
        let model = AgentComposerModel(
            teamId: composerTeamId,
            accountId: accountId,
            deps: deps,
            devicesModel: devicesModel,
            seed: seed
        )
        // EXP-1277: seed the options off the machine the pool has ALREADY
        // resolved — a composer rebuilt for the same machine would otherwise
        // never seed (`onChange(of: deviceId)` only fires on a change).
        model.reconcileDevice()
        Task { await model.load() }
        return model
    }

    /// EXP-1270/1316: while the team has NO board, Getting started sits
    /// ABOVE the dawn ("Start a run" focuses this page's composer).
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
        }
    }

    // MARK: - Captions

    /// What keeps the composer from starting (no machine, an agent the
    /// machine cannot drive, issues from several repositories…). EXP-1271:
    /// the wait is the card's starting row, a refused start a toast
    /// (FEED-79).
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
}
