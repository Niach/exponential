import ExpCore
import ExpUI
import GRDB
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The run's transcript and composer (EXP-32) — a chat-style view of a live
/// coding session over the relay's scrubbed activity channel. NO terminal
/// rendering: narration bubbles, compact tool rows, collapsible subagent
/// runs, question cards. Steering is message-shaped (text + \r, perm-gated by
/// the relay) and questions answer through the semantic `answer` frame
/// (EXP-249). Identical UX to the Android AgentSessionScreen.
///
/// EXP-893: a FACE of the Work screen, never a screen of its own. The nav
/// bar, its title dot, the Stop / Resume pill and the face switcher belong
/// to `WorkScreen`; this view reports what they need through `RunChrome`
/// and takes the screen's requests (`RunRequest`) and its switcher slot.
/// `face` picks the transcript (`.run`) or the run's live diff
/// (`.changes`, `SessionDiffList` + the Merge bar).
/// The feed's scroll constants — outside the view because it is generic over
/// its switcher slot (EXP-893) and a generic type cannot hold stored statics.
private enum AgentSessionLayout {
    static let bottomAnchor = "feed-bottom"
    static let feedCoordSpace = "feed-scroll"
    /// Within this many points of the bottom still counts as pinned (Android
    /// carries 96dp, EXP-529): a pixel-tight slack made the "Jump to bottom"
    /// pill hard to dismiss — a short drag had to land on the exact bottom to
    /// re-pin, so it lingered while the list visually WAS at the end (EXP-588).
    /// iOS gets a little more room than Android (EXP-591): the pill should
    /// only appear after a deliberate scroll-up and hide again well before
    /// the finger reaches the true end.
    static let followSlack: CGFloat = 120
}

struct AgentSessionView<Switcher: View>: View {
    let accountId: String
    let session: CodingSessionEntity
    let face: WorkFaceKind
    /// The screen's Stop pill asks; the kill confirm and the model are here.
    @Binding var request: RunRequest?
    /// A resumed / switched run's continuation row landed — the screen swaps
    /// it in place (EXP-773/849 pushed a second screen).
    let onContinuation: (StartedRunWatcher.StartedSession) -> Void
    @ViewBuilder let switcher: () -> Switcher

    @Environment(AppDependencies.self) private var deps
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL
    /// A cache of the SteerSessionStore lookup (EXP-621) — the model itself is
    /// app-scoped, so this view neither creates nor tears it down.
    @State private var model: AgentSessionModel?
    @State private var showKillConfirm = false
    /// EXP-688: the Usage sheet — the per-window cards that used to be a
    /// hairline strip under the nav bar. EXP-893: opened by the usage RING.
    @State private var showUsageSheet = false
    /// EXP-678: the Merge pill's confirm + in-flight call. No success state:
    /// the server ends the run and flips `pr_state`, and the pill disappears
    /// when that echo syncs back.
    @State private var showMergeConfirm = false
    @State private var merging = false
    /// EXP-706: the refusal AND whether the server diagnosed a REAL content
    /// conflict — the only case a retry can never fix, and the only one the
    /// recovery run can. A conflict swaps the Merge pill for "Fix conflicts".
    @State private var mergeFailure: MergeFailure?
    // "Fix conflicts" (EXP-323 rails, EXP-706 on this screen): the builtin
    // recovery run — EXP-825: NAVIGATION into the Agent page composer. The
    // watcher serves the account switch (EXP-849).
    @State private var steerEnabled = false
    @State private var startWatcher = StartedRunWatcher()
    /// Whether the feed is scrolled to (within slack of) its bottom —
    /// auto-scroll only while pinned; scrolling up pauses follow and surfaces
    /// the "Jump to bottom" pill.
    @State private var atBottom = true
    /// EXP-356: the selected conversation tab — nil is the main agent; a
    /// subagent id focuses that agent's stream. Falls back to Main whenever
    /// the id vanishes from the feed (an `activity_reset` replay).
    @State private var agentTab: String?
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showPhotoPicker = false
    /// EXP-849: a "switch account" is on the wire. It IS a resume naming
    /// another login, so it rides the same watcher and hands the run it
    /// produces — the continuation of this one — to the screen.
    @State private var switchingAccount = false
    /// EXP-724: the draft the `/` menu was dismissed at (Escape / an accepted
    /// row). Keyed on the DRAFT, not a bool, so the menu comes back on its own
    /// the moment the text changes and no `onChange` has to race the accept.
    @State private var slashDismissedFor: String?
    /// Keyboard-highlighted row of the `/` menu (hardware keyboards; ↑/↓ wrap).
    @State private var slashHighlight = 0
    /// A confirm-gated command waiting on its dialog (`/clear`).
    @State private var slashConfirm: SlashCommand?
    /// EXP-790: the composer is a folded capsule until it is tapped, and
    /// folds again on blur when nothing would be lost (IssueDetailBottomBar's
    /// rule). A non-empty draft or a pending image keeps it open regardless.
    @State private var composerExpanded = false
    /// EXP-820: per ask id, the wire id of the EARLIER step being re-answered
    /// ("go back" in the stepper), or absent while the current step shows.
    /// View state, not model state: it is a place in the card, and a fresh
    /// screen starts on the current step.
    @State private var editingSteps: [String: String] = [:]
    /// EXP-893: the latest diff's +/− counts, re-derived on the diff edge
    /// rather than scanned on every frame the feed repaints.
    @State private var diffAdditions = 0
    @State private var diffDeletions = 0
    /// EXP-895: the worktree diff through the ONE parser — read on the diff
    /// edge, never per frame.
    @State private var parsedDiff = Diff.Parsed(files: [])
    /// The phone's file list, off the Changes bar's leading slot, and the path
    /// it last picked.
    @State private var diffFileSheet = false
    @State private var selectedDiffPath: String?
    /// EXP-897: the synced rows behind the stack position line — the run's
    /// issue and every pull request around it.
    @State private var stackModel: PrGraphModel?
    @Environment(\.motion) private var motion

    /// EXP-746: Usage opens on EITHER half — the machine's rate-limit report
    /// (EXP-484) or this run's own context/spend off the relay. A fresh run on
    /// a machine that reported nothing used to have no usage affordance at all.
    private var hasUsage: Bool {
        // EXP-849: the readout is also the ACCOUNT surface, so a run whose
        // machine reported logins but no numbers still opens it.
        model?.runUsage != nil || model?.sessionUsage != nil
            || model?.accountOptions.isEmpty == false
    }

    // Three small chains instead of one long one. The whole modifier chain is
    // ONE expression to the type checker, and at this view's size that budget
    // has been blown twice already (#644, #656) — each time by a condition
    // spelled out inside one of its closures, and each fix bought exactly one
    // wave of headroom. Every group below is its own inference context, so a
    // new modifier costs its group and not the whole view. Nesting reads
    // inside out; the order of application is unchanged.
    var body: some View {
        withSheets(withLifecycle(withAlerts(sessionContent)))
    }

    private var sessionContent: some View {
        VStack(spacing: 0) {
            if let model {
                if face == .changes {
                    changesFace(model)
                } else {
                    runFace(model)
                }
            } else {
                Spacer()
            }
        }
    }

    /// The transcript face: the ended byline, the feed, the strips and the
    /// composer band.
    @ViewBuilder
    private func runFace(_ model: AgentSessionModel) -> some View {
        // EXP-849: a resumed/switched run says it is a continuation, above
        // everything else on the screen.
        continuationNote(model)
        // EXP-897: and where this run's pull request sits in its stack.
        stackPositionNote(model)
        // EXP-773: an ended run's close-out sits ABOVE its transcript.
        endedHeader(model)
        feedArea(model)
        banners(model)
        rateLimitBanner(model)
        compactionStrip(model)
        // EXP-850 §1/§2: the monitors and background shell commands,
        // directly above the composer. Absent when there is nothing running.
        AgentBottomStrip(lines: model.visibleStripLines)
        // EXP-861: the messages the device holds until the turn ends, each
        // with an X that revokes it. Absent when nothing is queued or the run
        // is over.
        if !model.queued.isEmpty, !model.isOver {
            AgentQueueStrip(messages: model.queued) { id in
                model.unqueue(id)
            }
        }
        bottomBar(model)
    }

    /// EXP-893: the Changes face — the run's latest worktree diff as a full
    /// page, with the Merge bar under it. The screen only shows this face
    /// while there IS a diff (it falls back to the issue's PR files, or to
    /// the Run face, when it vanishes).
    @ViewBuilder
    private func changesFace(_ model: AgentSessionModel) -> some View {
        if model.latestDiff != nil {
            // EXP-895: the raw `git diff` was read by the ONE parser the
            // moment it landed (`diffChanged`), so the face draws the same
            // cards every other Changes surface does.
            SessionDiffList(
                files: parsedDiff.files,
                truncatedLines: parsedDiff.truncatedLines,
                focusPath: selectedDiffPath
            )
            .safeAreaInset(edge: .bottom, spacing: 0) { changesFaceBar(model) }
            .sheet(isPresented: $diffFileSheet) {
                DiffFileListSheet(
                    files: parsedDiff.files,
                    selected: selectedDiffPath,
                    onSelect: { selectedDiffPath = $0 }
                )
            }
        } else {
            Spacer()
            changesFaceBar(model)
        }
    }

    /// The confirms: Kill, Merge, and a `/`-command's own.
    private func withAlerts(_ content: some View) -> some View {
        content
            .alert("Kill this coding session?", isPresented: $showKillConfirm) {
                Button("Kill session", role: .destructive) {
                    Task { await model?.killSession() }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This stops the agent on the desktop and ends the session.")
            }
            // EXP-678: merging from the steering screen — same confirm-gated flow
            // as Reviews.
            .alert("Merge pull request?", isPresented: $showMergeConfirm) {
                Button("Merge", role: .destructive) {
                    if let model { merge(model) }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                // EXP-734: this run's OWN pull request links no issue, so
                // promising completed issues would be a lie.
                if case .session = model?.mergeTarget {
                    Text("Merges this run's pull request and closes the coding session.")
                } else {
                    Text("Merges the pull request, completes every linked issue, and closes the coding session.")
                }
            }
            // EXP-724: `/clear` discards the conversation, so confirm rows
            // confirm before the frames go out. Copy is byte-identical ×4.
            .alert(
                slashConfirm.map { SlashCommands.confirmTitle($0) } ?? "",
                isPresented: Binding(
                    get: { slashConfirm != nil },
                    set: { if !$0 { slashConfirm = nil } }
                ),
                presenting: slashConfirm
            ) { command in
                Button(SlashCommands.confirmButton(command), role: .destructive) {
                    if let model { performSend(model) }
                }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text(SlashCommands.confirmBody)
            }
    }

    /// Attach/detach, the photo picker, and the state changes the screen
    /// reacts to. Their bodies live in methods below for the same reason the
    /// chain is split at all.
    private func withLifecycle(_ content: some View) -> some View {
        content
            // EXP-897: the stack line's rows. Keyed on the issue, so a
            // continuation that lands on another one re-arms.
            .task(id: (model?.session ?? session).issueId) {
                let stack = stackModel ?? PrGraphModel(accountId: accountId, db: deps.db)
                stackModel = stack
                stack.start(
                    issueId: (model?.session ?? session).issueId,
                    teamId: (model?.session ?? session).teamId
                )
            }
            .photosPicker(
                isPresented: $showPhotoPicker,
                selection: $photoItems,
                maxSelectionCount: SteerImageMessage.maxImages,
                matching: .images
            )
            .onChange(of: photoItems) { _, newItems in
                guard !newItems.isEmpty else { return }
                Task { await ingestPhotos(newItems) }
            }
            // EXP-790: blur collapses the composer ONLY when nothing would be
            // lost — empty draft, no pending images, no picker mid-flight
            // (presenting one resigns first responder). Copied from
            // IssueDetailBottomBar.
            // EXP-802: the composer's focus lives on its editor model now (a
            // UITextView owns first responder), not in a `@FocusState`.
            .onChange(of: model?.draftEditor.isEditing) { _, editing in
                draftEditingChanged(editing)
            }
            // EXP-893: the screen's Stop pill — the confirm is still this
            // view's, where the model is.
            .onChange(of: request) { _, request in
                requestChanged(request)
            }
            // EXP-893: the +/− counts follow the diff edge, not the frame.
            .onChange(of: model?.latestDiff, initial: true) { _, diff in
                diffChanged(diff)
            }
            // EXP-893: what the Work screen draws its nav bar and switcher
            // from. No scenePhase handler here: foreground revival (EXP-243)
            // is app-scoped since EXP-621.
            .preference(key: RunChrome.Key.self, value: runChrome)
            .onAppear {
                // The socket owner is app-scoped (EXP-621): popping back to this
                // screen re-attaches to the SAME model, so the feed is already
                // there, the composer still holds its draft, and there is no
                // connect phase to sit through.
                model = deps.steerSessions.attach(accountId: accountId, sessionId: session.id) {
                    AgentSessionModel(
                        accountId: accountId,
                        session: session,
                        currentUserId: deps.auth.userId,
                        steerApi: deps.steerApi,
                        attachmentsApi: deps.attachmentsApi,
                        issuesApi: deps.issuesApi,
                        db: deps.db
                    )
                }
                // EXP-909: the overlay's one-shot usage refresh rides the
                // device command queue. Set on every appearance — a reattached
                // model was built on an earlier one.
                model?.devicesApi = deps.devicesApi
            }
            .onDisappear {
                // NOT a teardown: the store keeps the socket up while the session
                // runs and retires it once it is over (or falls off the cap).
                deps.steerSessions.detach(accountId: accountId, sessionId: session.id)
                startWatcher.stop()
                // EXP-802: the DRAFT outlives this screen, its focus must not —
                // the editor model would otherwise hand first responder straight
                // back on return and pop the keyboard over a screen nobody typed
                // into. (The text is untouched; only the caret's claim goes.)
                model?.draftEditor.setFocused(nil)
            }
            // Steering on/off gates the recovery run.
            .task(id: accountId) {
                let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
                steerEnabled = config.enabled
            }
    }

    /// Sheets, and the continuation hand-off (EXP-849).
    private func withSheets(_ content: some View) -> some View {
        content
            // The desktop picked the switch up — hand the new run to the
            // screen ONCE, which swaps it in place of this one.
            .onChange(of: startWatcher.startedSession) { _, started in
                if let started {
                    startWatcher.startedSession = nil
                    onContinuation(started)
                }
            }
            // EXP-688: usage lives in its own sheet now — every window the machine
            // reported, grouped, instead of one pinned hairline.
            .sheet(isPresented: $showUsageSheet) {
                if hasUsage, let model {
                    AgentUsageSheet(
                        agent: model.session?.agent,
                        // EXP-909: the login the run SPENDS, and ITS windows —
                        // not the machine's active login's (the two differ the
                        // moment a run is started on a second account).
                        runAccount: model.runAccount,
                        account: model.agentAccount,
                        usage: model.runUsage,
                        sessionUsage: model.sessionUsage,
                        // EXP-849: the run's accounts, and the switch — a
                        // resume under another login, claude only, between
                        // turns, on the run's own machine.
                        accounts: model.accountOptions,
                        supportsSwitch: model.supportsAccountSwitch,
                        switchRefusal: { model.accountSwitchRefusal($0) },
                        switching: switchingAccount,
                        onSwitch: { option in
                            showUsageSheet = false
                            switchAccount(model, option)
                        },
                        // EXP-909: one refresh of the run's login per open.
                        onOpen: { model.requestUsageRefresh() }
                    )
                }
            }
    }

    // MARK: - Lifecycle handlers

    // EXP-790: blur collapses the composer ONLY when nothing would be lost —
    // empty draft, no pending images, no picker mid-flight (presenting one
    // resigns first responder). Copied from IssueDetailBottomBar.
    // EXP-802: the composer's focus lives on its editor model now (a UITextView
    // owns first responder), not in a `@FocusState`.
    private func draftEditingChanged(_ editing: Bool?) {
        guard composerExpanded, editing == false, let model else { return }
        guard !showPhotoPicker, photoItems.isEmpty else { return }
        guard model.trimmedDraft.isEmpty, model.pendingImages.isEmpty else { return }
        withAnimation(motion.standard) { composerExpanded = false }
    }

    /// EXP-893: the screen's request, consumed once.
    private func requestChanged(_ request: RunRequest?) {
        guard let request else { return }
        self.request = nil
        switch request {
        case .stop:
            if model?.canKill == true { showKillConfirm = true }
        }
    }

    /// EXP-895: the worktree diff is parsed ONCE, here, off the edge — the
    /// Changes face, the switcher's counts and the file sheet all read the
    /// same `Diff.Parsed`.
    private func diffChanged(_ diff: String?) {
        guard let diff else {
            parsedDiff = Diff.Parsed(files: [])
            selectedDiffPath = nil
            diffAdditions = 0
            diffDeletions = 0
            return
        }
        let parsed = Diff.parse(diff)
        parsedDiff = parsed
        let totals = Diff.totals(parsed.files)
        diffAdditions = totals.additions
        diffDeletions = totals.deletions
    }

    // MARK: - Chrome report (EXP-893)

    /// Whether the socket is on its way — the title dot pulses.
    private var isConnecting: Bool {
        switch model?.phase {
        case .starting, .connecting, .idle, .none: return true
        case let .closed(_, reconnecting): return reconnecting
        default: return false
        }
    }

    /// Everything the Work screen's nav bar, title dot and switcher read.
    private var runChrome: RunChrome {
        guard let model else { return RunChrome() }
        var chrome = RunChrome()
        chrome.live = model.phase == .live
        chrome.connecting = isConnecting
        chrome.paused = hostPaused || headerLost
        chrome.awaitingInput = model.awaitingInput
        chrome.stale = model.staleActivityMinutes != nil
        chrome.busy = model.agentWorking
        chrome.over = model.isOver
        chrome.cardPending = model.cardPending
        chrome.hasDiff = model.latestDiff != nil
        chrome.additions = diffAdditions
        chrome.deletions = diffDeletions
        chrome.canMerge = model.canMerge
        chrome.canKill = model.canKill
        chrome.continuationPending = startWatcher.sentCaption != nil || switchingAccount
        return chrome
    }

    // MARK: - Header

    /// Everything an `AgentMarkdownText` needs to render embedded images —
    /// agent prose can carry `![](/api/attachments/{id})` and those fetches are
    /// authenticated (EXP-440).
    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            // EXP-760: narration, steered messages, plans and question prompts
            // chip issue identifiers the agent names — `#EXP-1` AND the bare
            // `EXP-1` agents actually write — scoped to this run's team, since
            // a batch / action / chat run has no issue to derive one from.
            issueRefs: AgentIssueRefContext(
                teamId: session.teamId,
                db: deps.db,
                onOpen: { issueId in
                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                }
            )
        )
    }

    /// EXP-550: the host machine is asleep and the run is parked on it — the
    /// screen says paused (grey), not "Connecting…" forever.
    private var hostPaused: Bool {
        guard model?.hostDeviceOffline == true else { return false }
        switch model?.phase {
        case .starting, .connecting, .idle, .none: return true
        case let .closed(_, reconnecting): return reconnecting
        default: return false
        }
    }

    /// EXP-549: the CURRENT machine label off the synced devices row — the
    /// session's own `device_label` is a start-time snapshot a rename never
    /// rewrites.
    private var hostLabel: String {
        model?.hostDevice.displayLabel
            ?? SessionDevicePresentation.resolve(session: session, devices: []).displayLabel
    }

    /// The socket is gone for good as far as this screen is concerned — a
    /// dropped connection or an ended run. The dot goes static neutral with
    /// the paused ones: none of them is "coding now".
    private var headerLost: Bool {
        switch model?.phase {
        case .ended, .closed: return true
        default: return false
        }
    }

    /// EXP-773: the journal fetch's own status line — which machine is being
    /// asked, and what it said. Nil whenever no fetch is in play. Mirrored ×4.
    private func historyStatus(_ model: AgentSessionModel) -> String? {
        switch model.history {
        case .pending: return "Fetching the transcript from \(hostLabel)…"
        case .deviceOffline:
            return "\(hostLabel) is offline. The transcript lives on that machine."
        case .unavailable: return "No transcript on \(hostLabel)."
        case nil: return nil
        }
    }

    // MARK: - Ended header (EXP-773)

    /// A finished run's byline above the transcript, and the account
    /// switch's progress captions. EXP-893: Resume moved to the Work
    /// screen's nav bar; EXP-862 dropped the close-out summary — the
    /// transcript right below is what a finished run has to say.
    @ViewBuilder
    private func endedHeader(_ model: AgentSessionModel) -> some View {
        if model.sessionEnded {
            VStack(alignment: .leading, spacing: 8) {
                Text(endedByline(model))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let failure = startWatcher.failure {
                    Text(failure)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } else if let caption = startWatcher.sentCaption {
                    Text(caption)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
        }
    }

    /// "macbook · 5m ago" — the ×4 `PastRuns` rule the list rows print, now
    /// that the row itself only carries a link.
    private func endedByline(_ model: AgentSessionModel) -> String {
        let row = model.session ?? session
        return PastRuns.byline(
            device: model.hostDevice.displayLabel,
            relativeTime: relativeWireDate(PastRuns.endedAt(row))
        )
    }

    /// EXP-849: change the account this run uses — a RESUME on the same
    /// machine naming another login profile
    /// (`steer.startSession({ resumeSessionId, deviceId, account })`). The
    /// device relaunches the run under that profile's config dir and the agent
    /// re-reads this run's transcript there, once, on the account moved to
    /// (`SessionAccountSwitch.costNote`).
    ///
    /// A start is a COMMAND, so the shared watcher waits for the row the
    /// desktop inserts (linked by `resumed_from_id`) and hands that session to
    /// the screen, which swaps it in as this run's continuation. The wall
    /// notice that prompted the switch goes with it.
    private func switchAccount(_ model: AgentSessionModel, _ option: SessionAccountOption) {
        guard model.accountSwitchRefusal(option) == nil, !switchingAccount else {
            return
        }
        guard let device = model.switchDevice else { return }
        switchingAccount = true
        startWatcher.sending()
        model.clearRateLimit()
        Task {
            do {
                try await deps.steerApi.resumeSession(
                    accountId: accountId,
                    sessionId: session.id,
                    deviceId: device.deviceId,
                    // The picked profile VERBATIM, `system` included: the server
                    // reads the PRESENCE of `account` as "this resume is a
                    // switch", which is the only thing that lets a resume ride
                    // a LIVE run — the ×4 `wireAccount` rule.
                    account: SessionAccountSwitch.wireAccount(option)
                )
                startWatcher.begin(
                    key: .resumed(fromId: session.id),
                    userId: deps.auth.userId,
                    device: device,
                    db: deps.db,
                    accountId: accountId
                )
            } catch {
                startWatcher.failed(error.userFacingMessage)
            }
            switchingAccount = false
        }
    }

    /// EXP-849: this run IS the continuation of an earlier one (a Resume, or a
    /// switch to another account) — say so once, with the transcript's one-time
    /// cost, so a second context-window charge on a new account is never a
    /// surprise. The ×4 sentence; the run it continues is reachable from the
    /// lists, which nest the chain.
    @ViewBuilder
    private func continuationNote(_ model: AgentSessionModel) -> some View {
        if let resumedFrom = (model.session ?? session).resumedFromId, !resumedFrom.isEmpty {
            HStack(alignment: .top, spacing: 6) {
                AppIcon(AppIcons.runResume, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                VStack(alignment: .leading, spacing: 2) {
                    Text(SessionAccountSwitch.continuationNote)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    // The one-time transcript re-read, said ONCE on the run it
                    // cost — a second context charge on a new account must
                    // never be a surprise.
                    Text(SessionAccountSwitch.continuationCostNote)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .accessibilityIdentifier("run-continuation")
        }
    }

    // MARK: - Stack position (EXP-897)

    /// The run's issue and the pull requests around it.
    private var stackGraph: PrGraph.Graph? {
        guard let stackModel,
              let issueId = (model?.session ?? session).issueId,
              let issue = stackModel.issue(id: issueId)
        else { return nil }
        return stackModel.graph(issue: issue, session: nil)
    }

    /// `2 of 3 · on top of #ABC-12` — where this run's pull request sits in
    /// its stack, with a step down to its foundation and up to what is built
    /// on it. The ×4 line; both steps open that pull request's Changes.
    @ViewBuilder
    private func stackPositionNote(_ sessionModel: AgentSessionModel) -> some View {
        if let graph = stackGraph, let label = graph.positionLabel {
            HStack(spacing: 6) {
                AppIcon(AppIcons.prStack, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(stackPositionLine(label: label, below: graph.below))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                Spacer(minLength: 0)
                if let below = graph.below {
                    stackStep(AppIcons.uiChevronDown, label: "Open the pull request below", entry: below)
                }
                if let above = graph.above {
                    stackStep(AppIcons.uiChevronUp, label: "Open the pull request above", entry: above)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .accessibilityIdentifier("run-stack-position")
        }
    }

    private func stackPositionLine(label: String, below: PrGraph.Entry?) -> String {
        guard let identifier = below?.representative.identifier, !identifier.isEmpty else {
            return label
        }
        return "\(label) · on top of #\(identifier)"
    }

    @ViewBuilder
    private func stackStep(_ glyph: String, label: String, entry: PrGraph.Entry) -> some View {
        Button {
            pushRoute(.changes(accountId: accountId, issueId: entry.representative.id))
        } label: {
            AppIcon(glyph, size: 12)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(width: 22, height: 22)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    // MARK: - Feed

    @ViewBuilder
    private func feedArea(_ model: AgentSessionModel) -> some View {
        if model.feed.isEmpty, let status = historyStatus(model) {
            // EXP-773: a finished run's transcript lives on the machine that
            // ran it, and the relay is asking that machine for it. Say which
            // machine, and say plainly when it can't answer — this is not a
            // connection problem the viewer can wait out.
            centeredState {
                if model.history == .pending {
                    ProgressView().tint(.white)
                }
                Text(status)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .multilineTextAlignment(.center)
            }
        } else if model.feed.isEmpty, hostPaused {
            // EXP-550: the machine hosting this run is asleep. The session is
            // NOT over — it resumes when the machine comes back — so say that
            // instead of spinning on "waiting for the live stream" forever.
            centeredState { hostOfflineState }
        } else if model.feed.isEmpty,
                  model.phase == .connecting || model.phase == .starting || model.phase == .idle {
            centeredState {
                ProgressView().tint(.white)
                Text(model.phase == .starting
                    ? "The agent is starting. Waiting for the live stream…"
                    : "Connecting…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .multilineTextAlignment(.center)
            }
        } else if model.feed.isEmpty, model.phase == .live, model.latestDiff == nil {
            centeredState {
                Text("Waiting for activity…")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("Update the Exponential desktop app to see the live feed.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)
            }
        } else {
            VStack(spacing: 0) {
                agentTabStrip(model)
                feedList(model)
            }
            // Only present once real activity has arrived — the store-screenshot
            // test waits on it so it never captures a placeholder state.
            // `children: .contain` keeps the feed's own elements queryable; a
            // bare identifier on a plain container never reaches the hierarchy.
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-feed")
        }
    }

    /// EXP-356: conversation tabs — Main plus one per RUNNING subagent (ended
    /// tabs are dropped, EXP-387). Rendered only while a tab is visible; the
    /// strip scrolls horizontally on a fan-out.
    @ViewBuilder
    private func agentTabStrip(_ model: AgentSessionModel) -> some View {
        let agents = AgentFeed.visibleSubagentTabs(
            model.subagents, selected: agentTab
        )
        if !agents.isEmpty {
            let active = agents.contains { $0.subagentId == agentTab } ? agentTab : nil
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    agentTabChip(label: "Main", running: false, selected: active == nil) {
                        agentTab = nil
                    }
                    ForEach(agents) { run in
                        // EXP-847: what the spawn ASKED for, with the agent
                        // type as the fallback.
                        agentTabChip(
                            label: run.label,
                            running: !run.done,
                            selected: active == run.subagentId
                        ) {
                            agentTab = run.subagentId
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
            }
        }
    }

    private func agentTabChip(
        label: String,
        running: Bool,
        selected: Bool,
        action: @escaping () -> Void
    ) -> some View {
        GlassPill(label, mode: .select(isSelected: selected, action: action)) {
            if running {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
            }
        }
    }

    private func centeredState(@ViewBuilder content: () -> some View) -> some View {
        VStack(spacing: 8) {
            content()
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Bottom-anchored feed (a short feed sits above the input bar, not at the
    /// top of the screen) with follow-scroll: pinned to the bottom until the
    /// user scrolls up, then a "Jump to bottom ↓" pill re-pins.
    ///
    /// Follow state is derived from scroll GEOMETRY and the pin is an explicit
    /// scrollTo — NOT from onAppear/onDisappear of a lazy sentinel and NOT
    /// from defaultScrollAnchor(.bottom). Both of those make layout depend on
    /// state the layout itself mutates (lazy realization ⇄ body invalidation,
    /// anchored re-scroll ⇄ lazy sizing), and once the feed outgrew the
    /// viewport that cycle wedged the main thread for good (EXP-70). Geometry
    /// comes from onScrollGeometryChange on iOS 18+ — the EXP-70 content-frame
    /// preference stopped updating during scrolls on current iOS, which left
    /// `atBottom` stuck true and the pill never appeared (EXP-212); the
    /// preference stays as the pre-18 fallback.
    private func feedList(_ model: AgentSessionModel) -> some View {
        GeometryReader { geo in
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        // EXP-356: a selected agent tab focuses that subagent's
                        // conversation; Main keeps the grouped feed.
                        if let focused = agentTab.flatMap({ id in
                            model.subagents.first { $0.subagentId == id }
                        }) {
                            SubagentRow(
                                agentType: focused.agentType,
                                title: focused.title,
                                status: focused.done ? .completed : .started,
                                detail: focused.detail
                            )
                            if focused.items.isEmpty {
                                Text("Nothing from this agent yet.")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .padding(.vertical, 4)
                            } else {
                                // EXP-773: the run's prose and the turns
                                // addressed to it interleave with its tool
                                // calls, in publish order. EXP-916: its edit
                                // runs project into edited-files cards, exactly
                                // as they do in the main transcript.
                                let laneRows = AgentFeed.laneRows(focused.items)
                                ForEach(
                                    Array(laneRows.enumerated()), id: \.element.id
                                ) { index, row in
                                    SubagentLaneRow(
                                        row: row, context: markdownContext, nested: false
                                    )
                                    .padding(.top, Self.gapPoints(AgentFeed.transcriptGap(
                                        // The SubagentRow header above the
                                        // list is the first "previous row".
                                        prev: index == 0 ? .tool : laneRows[index - 1].rowClass,
                                        cur: row.rowClass
                                    )))
                                }
                            }
                        } else {
                            // Consecutive tool calls collapse into "N tool
                            // calls" rows (EXP-97) — a render projection; the
                            // flat feed (and the trailing-question rule) stays
                            // the state.
                            // EXP-783: the run's transcript is kept WHOLE and
                            // rendered as a window; "Load earlier" grows it
                            // upward (and, past the feed's own first row, asks
                            // the device for the page below it).
                            if model.canLoadEarlier {
                                Button("Load earlier") { model.loadEarlier() }
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                                    .frame(maxWidth: .infinity)
                                    .padding(.bottom, 4)
                            }
                            let rows = model.rows
                            // EXP-787: every row's space above it comes from
                            // the shared gap ladder, keyed on the row BEFORE
                            // it — the rows themselves carry no outer margin
                            // any more, so the rhythm is one derivation
                            // instead of a padding per row kind.
                            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                                feedRow(row, isLast: row.id == rows.last?.id)
                                    .padding(.top, Self.gapPoints(AgentFeed.transcriptGap(
                                        prev: index == 0 ? nil : rows[index - 1].rowClass,
                                        cur: row.rowClass
                                    )))
                            }
                            // EXP-389: the agent-is-busy footer. EXP-848: the
                            // model's ONE working predicate — it used to read a
                            // looser local copy, so this row could pulse while
                            // the composer showed Send.
                            if model.agentWorking {
                                // EXP-850 §5: the turn's verb (or the running
                                // workflow's caption) with its duration and
                                // token count, beside the agent's own pulsing
                                // brand mark.
                                WorkingIndicatorRow(
                                    agent: (model.session ?? session).agent,
                                    caption: { model.workingCaption(now: $0) }
                                )
                                    .padding(.top, Self.gapPoints(AgentFeed.transcriptGap(
                                        prev: rows.last?.rowClass, cur: .tool
                                    )))
                            }
                        }
                        Color.clear
                            .frame(height: 1)
                            .id(AgentSessionLayout.bottomAnchor)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    // EXP-787: a transcript is a reading measure. The 16pt
                    // screen inset stays, but the column caps at the shared
                    // `maxWidth` and centres inside whatever is wider — a
                    // landscape phone or an iPad must not stretch prose edge
                    // to edge.
                    .frame(maxWidth: DesignTokens.Transcript.maxWidth, alignment: .leading)
                    .frame(maxWidth: .infinity)
                    .frame(minHeight: geo.size.height, alignment: .bottom)
                    .background(
                        GeometryReader { content in
                            Color.clear.preference(
                                key: FeedBottomOverflowKey.self,
                                value: content.frame(in: .named(AgentSessionLayout.feedCoordSpace)).maxY
                                    - geo.size.height
                            )
                        }
                    )
                }
                // EXP-698: the nav bar is `.ultraThinMaterial`, so a scrolled
                // narration line used to be sliced through its letterforms at
                // the header's edge. The fade lets it recede instead.
                .stickyHeaderFade()
                .coordinateSpace(name: AgentSessionLayout.feedCoordSpace)
                .modifier(FollowPinTracker(
                    atBottom: $atBottom,
                    slack: AgentSessionLayout.followSlack,
                    repin: { proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom) }
                ))
                .onAppear {
                    proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                    // Lazy rows can still be sizing on the first pass, landing
                    // the scroll short — re-assert once layout has settled.
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(50))
                        if atBottom {
                            proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                        }
                    }
                }
                .onChange(of: model.feed.count) { _, _ in
                    if atBottom {
                        proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                    }
                }
                // Switching conversation tabs re-pins to the newest event
                // (EXP-356).
                // EXP-795: back at the bottom, the window slides with the stream
                // again (web/Android/desktop parity).
                .onChange(of: atBottom) { _, now in model.noteAtBottom(now) }
                .onChange(of: agentTab) { _, _ in
                    atBottom = true
                    proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                }
                .overlay(alignment: .bottom) {
                    if !atBottom {
                        // Opaque: the feed scrolls beneath this pill
                        // (EXP-165 Android parity, EXP-242).
                        GlassPill(
                            "Jump to bottom ↓",
                            size: .md,
                            mode: .select(isSelected: true) {
                                // Re-arm follow directly (Android parity: the
                                // pill tap sets follow=true) instead of waiting
                                // for the scroll geometry to flip atBottom —
                                // while the agent streams, the animated
                                // scrollTo targets the bottom as of the tap,
                                // the content keeps growing underneath it, and
                                // the animation lands short of the moving max
                                // offset, so the pill never vanished
                                // (EXP-306). With atBottom set here the pill
                                // hides at once and the growth observer keeps
                                // chasing the bottom.
                                atBottom = true
                                withAnimation {
                                    proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                                }
                            },
                            isOpaque: true
                        )
                        .padding(.bottom, 8)
                    }
                }
            }
        }
    }

    /// EXP-787: the ladder's token name resolved to points. ExpCore cannot see
    /// ExpUI, so `AgentFeed.transcriptGap` names the gap and the view measures
    /// it off the shared `DesignTokens.Transcript` group.
    private static func gapPoints(_ gap: AgentTranscriptGap) -> CGFloat {
        switch gap {
        case .none: 0
        case .turn: DesignTokens.Transcript.gapTurn
        case .block: DesignTokens.Transcript.gapBlock
        case .tool: DesignTokens.Transcript.gapTool
        case .default: DesignTokens.Transcript.gapDefault
        }
    }

    @ViewBuilder
    private func feedRow(_ row: AgentFeedRow, isLast: Bool) -> some View {
        switch row {
        case let .toolRun(items):
            ToolGroupRow(
                items: items,
                liveTail: isLast && model?.phase == .live,
                // EXP-846: an Exponential call's issue preview resolves and
                // navigates against the run's team.
                refs: markdownContext.issueRefs
            )
        case let .edits(items):
            // EXP-916: a run of edit calls is ONE card — the same file cards
            // the Changes face draws, stacked flush, the live one open.
            EditedFilesCard(items: items, liveItemId: liveToolRowId)
        case let .subagentRun(run):
            SubagentGroupRow(
                run: run,
                liveTail: isLast && model?.phase == .live,
                context: markdownContext
            )
        case let .ask(group):
            askCard(group)
        case let .single(item):
            switch item {
            case let .narration(_, text, _, _):
                NarrationBubble(text: text, context: markdownContext)
            case let .tool(
                id, name, detail, _, callId, _, settled, failed, diff, preview, output
            ):
                toolOrWorkflowRow(
                    item: item, name: name, detail: detail, callId: callId,
                    settled: settled, failed: failed, diff: diff,
                    preview: preview, output: output, live: id == liveToolRowId
                )
            case let .userMessage(_, text, _):
                // EXP-724: a steered slash command is a control action, not
                // prose — it renders as a compact pill instead of a bubble.
                // EXP-746: over the MERGED catalog, so a command the agent
                // itself advertised gets the pill too (the `/` menu that sent
                // it reads the same merge).
                if let command = SlashCommands.command(
                    for: text,
                    agent: model?.catalogAgent,
                    extra: model?.sessionConfig?.commands ?? []
                ) {
                    CommandPill(command: command, text: text)
                } else {
                    UserMessageBubble(text: text, context: markdownContext)
                }
            case let .question(question):
                questionCard(question)
            case let .subagent(_, _, agentType, status, detail, _, title, _):
                SubagentRow(
                    agentType: agentType, title: title, status: status, detail: detail
                )
            case let .permission(_, tool, detail):
                PermissionRow(tool: tool, detail: detail)
            case .compaction:
                CompactionMarkerRow()
            }
        }
    }

    /// EXP-850 §3: a `Workflow` call renders as its CARD — the latest-wins
    /// `workflow` event with the same id — never as a tool row plus a second
    /// card row. EXP-916: a call that is no card member and yet carries a PATCH
    /// draws a one-member edited-files card; everything else is the ordinary
    /// tool row.
    ///
    /// Its own method, not a branch inside `feedRow`: that switch is ONE
    /// expression to the type checker and spelling this out inline blew its
    /// budget outright ("failed to produce diagnostic for expression").
    @ViewBuilder
    private func toolOrWorkflowRow(
        item: AgentFeedItem,
        name: String,
        detail: String?,
        callId: String?,
        settled: Bool,
        failed: Bool,
        diff: String?,
        preview: AgentToolPreview?,
        output: String?,
        live: Bool
    ) -> some View {
        if let workflow: AgentWorkflow = model?.workflow(for: callId) {
            AgentWorkflowCardRow(
                workflow: workflow,
                runFor: workflowAgentRun,
                context: markdownContext
            )
        } else if let diff, !diff.isEmpty {
            // EXP-916: a call outside an edited-files card may still CARRY a
            // patch (an edit tagged with a workflow). The card is the only
            // place a patch renders now, so a ONE-member card draws it rather
            // than a row that drops it.
            EditedFilesCard(
                items: [item], liveItemId: live ? item.id : nil
            )
        } else {
            ToolRow(
                name: name, detail: detail, failed: failed,
                settled: settled, preview: preview,
                refs: markdownContext.issueRefs, output: output, live: live
            )
        }
    }

    /// EXP-895: the ONE row the transcript keeps EXPANDED — the last feed item
    /// while it is an unsettled tool call, and only in a LIVE run (an ended
    /// run's trailing unsettled row is history, not a tail). `AgentFeed`'s rule,
    /// shared ×4.
    private var liveToolRowId: Int? {
        guard model?.phase == .live, let feed = model?.feed else { return nil }
        return AgentFeed.liveToolRowId(feed)
    }

    /// The nested run behind one workflow agent — a method rather than a
    /// closure literal at the call site, for the same budget reason.
    private func workflowAgentRun(_ agentId: String?) -> AgentSubagentRun? {
        model?.subagentRun(agentId: agentId)
    }

    /// A lone question card: a plan approval or a single-question ask.
    @ViewBuilder
    private func questionCard(_ question: AgentQuestion) -> some View {
        QuestionCard(
            question: question,
            localAnswer: model?.localAnswerSummary(question.lockKey),
            active: model?.activeQuestionIds.contains(question.id) ?? false,
            canAnswer: canAnswer,
            locked: model?.isAnswerLocked(question.lockKey) ?? false,
            pending: model?.isAnswerPending(question.lockKey) ?? false,
            failed: model?.isAnswerFailed(question.lockKey) ?? false,
            onAnswer: { keys, text in sendAnswer(question, keys: keys, text: text) },
            onAnswerThenSend: { keys, text in answerThenSend(question, keys: keys, text: text) },
            markdownContext: markdownContext
        )
        .id(question.id)
        // EXP-642: the store slide's pop-out rect is measured off the question
        // card (`PopRects`). `contain` keeps its option buttons queryable.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-feed-question")
    }

    /// A multi-question ask (EXP-249) as ONE stepper card, claude-style: one
    /// step at a time, the desktop's `answer_ack` advances it, and the ask's
    /// final review step submits the whole thing.
    ///
    /// EXP-820 "go back": while the ask is still open (`AgentFeed.askComplete`
    /// false) an answered step row is tappable and swaps THAT step back in —
    /// its options answerable again past its lock (`reanswer`), the recorded
    /// answer pre-selected, the current step folded into a muted row — and
    /// picking (or "Back to current step") returns to the current step. The
    /// engine re-records the step without moving the stepper.
    @ViewBuilder
    private func askCard(_ group: AgentAskGroup) -> some View {
        let stepIndex = AgentFeed.currentStepIndex(
            of: group, done: model?.answeredQuestionIds ?? []
        )
        // Every step done: keep the last one on screen with its resolution.
        let index = stepIndex ?? (group.questions.count - 1)
        let complete = AgentFeed.askComplete(group)
        // Editing is offered only while the ask is open and this client may
        // answer at all; a stale editing id (the ask completed, or the step
        // moved past `index`) simply falls back to the current step.
        let editable = canAnswer && !complete
        let editHandler: ((String) -> Void)? = editable
            ? { wireId in editingSteps[group.askId] = wireId }
            : nil
        let answered = Array(group.questions.prefix(index))
        let editingStep: AgentQuestion? = editable
            ? editingSteps[group.askId].flatMap { id in answered.first { $0.wireId == id } }
            : nil
        if group.questions.indices.contains(index) {
            let current = group.questions[index]
            if let editingStep {
                // The OTHER answered steps stay listed (each still tappable, so
                // the steerer can hop between them); the edited one is the
                // card's prompt.
                let others = answered.filter { $0.wireId != editingStep.wireId }
                QuestionCard(
                    question: editingStep,
                    stepLabel: stepLabel(for: editingStep, in: group),
                    priorSteps: others,
                    priorAnswers: others.map { step in
                        step.answerSummary ?? model?.localAnswerSummary(step.lockKey)
                    },
                    localAnswer: model?.localAnswerSummary(editingStep.lockKey),
                    active: true,
                    canAnswer: canAnswer,
                    // Answerable past its lock — `reanswer` on the send.
                    locked: false,
                    pending: false,
                    failed: false,
                    askOpen: true,
                    editing: true,
                    editingAnswer: editingStep.answerSummary
                        ?? model?.localAnswerSummary(editingStep.lockKey),
                    foldedCurrent: current,
                    onEditStep: editHandler,
                    onExitEditing: { editingSteps[group.askId] = nil },
                    onAnswer: { keys, text in
                        sendAnswer(editingStep, keys: keys, text: text, reanswer: true)
                        editingSteps[group.askId] = nil
                    },
                    markdownContext: markdownContext
                )
                // Its own identity per edited step — the picked state starts
                // from the recorded answer, never from the current step's.
                .id("edit-\(editingStep.id)")
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("agent-feed-question")
            } else {
                QuestionCard(
                    question: current,
                    stepLabel: stepLabel(for: current, in: group),
                    priorSteps: answered,
                    priorAnswers: answered.map { step in
                        step.answerSummary ?? model?.localAnswerSummary(step.lockKey)
                    },
                    localAnswer: model?.localAnswerSummary(current.lockKey),
                    active: stepIndex != nil && (model?.activeQuestionIds.contains(current.id) ?? false),
                    canAnswer: canAnswer,
                    locked: model?.isAnswerLocked(current.lockKey) ?? false,
                    pending: model?.isAnswerPending(current.lockKey) ?? false,
                    failed: model?.isAnswerFailed(current.lockKey) ?? false,
                    askOpen: !complete,
                    onEditStep: editHandler,
                    onAnswer: { keys, text in sendAnswer(current, keys: keys, text: text) },
                    markdownContext: markdownContext
                )
                // A fresh identity per step — the card's local selection state must
                // never leak from one question into the next.
                .id(current.id)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("agent-feed-question")
            }
        }
    }

    private func stepLabel(for question: AgentQuestion, in group: AgentAskGroup) -> String? {
        guard let index = question.index else { return nil }
        let total = question.total ?? group.stepCount
        return total > 1 ? "Question \(index) of \(total)" : nil
    }

    /// The picked labels (a typed free-text reply wins over its row's "Type
    /// something" label) — what the stepper shows for this step until the
    /// desktop resolves the ask (EXP-588).
    private func answerLabels(_ question: AgentQuestion, keys: [String], text: String?) -> [String] {
        keys.compactMap { key in
            guard let option = question.options.first(where: { $0.key == key }) else { return nil }
            if option.freeText, let text, !text.isEmpty { return text }
            return option.label
        }
    }

    private func sendAnswer(
        _ question: AgentQuestion, keys: [String], text: String? = nil, reanswer: Bool = false
    ) {
        model?.sendAnswer(
            questionId: question.wireId, askId: question.askId,
            keys: keys, text: text, labels: answerLabels(question, keys: keys, text: text),
            reanswer: reanswer
        )
    }

    /// EXP-820: a plan's inline feedback — deny on the reject row, then the
    /// text as the next message (`AgentSessionModel.answerThenSend`).
    private func answerThenSend(_ question: AgentQuestion, keys: [String], text: String) {
        model?.answerThenSend(
            questionId: question.wireId, askId: question.askId,
            keys: keys, labels: answerLabels(question, keys: keys, text: nil), text: text
        )
    }

    /// Whether this client may answer questions at all — a question card is
    /// answerable when this holds AND it is still active per
    /// `activeQuestionIds` (EXP-78/EXP-174).
    private var canAnswer: Bool {
        guard let model else { return false }
        return model.phase == .live && !model.sessionEnded
    }

    // MARK: - Status banners (feed retained above)

    /// EXP-804: the usage wall used to ride beside the nav-bar caption; the
    /// Work screen's title has no caption line, so it is a banner here.
    private func blockedLabel(_ model: AgentSessionModel) -> String? {
        AgentUsagePresentation.blockedBadgeLabel(
            AgentUsagePresentation.parseBlocked((model.session ?? session).blocked),
            now: model.activityNow
        )
    }

    @ViewBuilder
    private func banners(_ model: AgentSessionModel) -> some View {
        if blockedLabel(model) != nil {
            bannerRow {
                SessionBlockedBadge(blocked: (model.session ?? session).blocked)
            }
        }
        // Kill-switch failure (EXP-268) — inline banner; cleared on retry.
        if let killError = model.killError {
            bannerRow {
                Text("Couldn't kill the session. \(killError)")
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        // A refused merge (conflicts, branch protection) — same shape
        // (EXP-678); cleared on the next attempt. EXP-706: the reason only —
        // a conflict's recovery run took the Merge pill's slot in the bar.
        if let mergeFailure {
            bannerRow {
                Text(mergeFailure.message)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        // The switch's own progress (EXP-536): "sent to <machine>", then the
        // continuation swaps in once the desktop picks it up.
        if let runCaption = startWatcher.sentCaption {
            bannerRow {
                Text(runCaption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
        if let runError = startWatcher.failure {
            bannerRow {
                Text(runError)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        // EXP-550: an offline host outranks every reconnect/starting banner —
        // the redial loops keep running underneath (untouched), they just
        // stop being what the viewer is told about.
        if hostPaused, !model.feed.isEmpty {
            bannerRow {
                AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("\(hostLabel) is offline. The agent is paused on that machine and continues when it comes back online.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        } else {
            phaseBanner(model)
        }
    }

    @ViewBuilder
    private func phaseBanner(_ model: AgentSessionModel) -> some View {
        switch model.phase {
        case let .ended(detail):
            bannerRow {
                Text(detail ?? "Session ended")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        case let .closed(detail, reconnecting):
            bannerRow {
                if reconnecting {
                    ProgressView().controlSize(.small).tint(.white)
                }
                Text(detail ?? (reconnecting ? "Connection lost. Reconnecting…" : "Disconnected"))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        case .starting where !model.feed.isEmpty:
            bannerRow {
                ProgressView().controlSize(.small).tint(.white)
                Text("The agent is starting. Waiting for the live stream…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        default:
            EmptyView()
        }
    }

    /// EXP-550: the "your machine is asleep" empty state — the same glyph the
    /// machines list draws for an offline device (`ui-device-offline`).
    @ViewBuilder
    private var hostOfflineState: some View {
        AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.large)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
        Text("\(hostLabel) is offline")
            .font(.subheadline)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
        Text("The agent is paused on that machine and continues when it comes back online.")
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .multilineTextAlignment(.center)
    }

    // MARK: - Compaction strip (EXP-724)

    /// The indeterminate strip a running compaction puts above the composer:
    /// the agent goes silent for 10–170s while it folds its context away, and
    /// without this the viewer reads that as a hang. Indeterminate on purpose
    /// — no publisher knows how far along a compaction is. It never outlives
    /// the session (the model clears the state on every end path).
    @ViewBuilder
    private func compactionStrip(_ model: AgentSessionModel) -> some View {
        if model.compacting != nil, !model.isOver {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    AppIcon(AppIcons.codingCompact, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text(AgentFeed.compactingLabel)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Spacer(minLength: 0)
                }
                ProgressView()
                    .progressViewStyle(.linear)
                    .tint(.white.opacity(TextOpacity.secondary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .padding(.horizontal, 14)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(AgentFeed.compactingLabel)
        }
    }

    // MARK: - Rate-limit banner (EXP-784)

    /// The agent's rate-limit window, in the compaction strip's slot: its own
    /// message and when the window resets, local time. It stands only while
    /// the slot holds a WALL (EXP-818: a warning over a working run is not
    /// one) whose reset has not passed (EXP-831: it re-reads the model's 30s
    /// activity clock and drops itself a minute past the stamp) — an
    /// `ok`/empty status, the replay swap and the end of the run all clear it.
    @ViewBuilder
    private func rateLimitBanner(_ model: AgentSessionModel) -> some View {
        if let limit = model.sessionRateLimit, !model.isOver,
           AgentFeed.rateLimitBannerShows(limit, now: model.activityNow) {
            let caption = AgentFeed.rateLimitCaption(limit)
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                    .foregroundStyle(DesignTokens.Semantic.yellow)
                Text(caption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                // EXP-849: the wall's PRIMARY answer — the other account. It
                // opens the readout's account rows (bars and health included),
                // where the switch itself is confirmed per account; a run whose
                // machine reports one login has nothing to offer and keeps the
                // bare notice.
                // Absent unless a switch is actually possible, so the wall
                // never offers a dead end.
                if model.canSwitchAnyAccount {
                    GlassPill(
                        SessionAccountSwitch.wallSwitchLabel,
                        icon: AppIcons.uiSwap,
                        mode: .action { showUsageSheet = true },
                        primary: true
                    )
                    .accessibilityIdentifier("rate-limit-switch-account")
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .padding(.horizontal, 14)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(caption)
            .accessibilityIdentifier("agent-rate-limit")
        }
    }

    private func bannerRow(@ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 8) {
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    // MARK: - Bottom bar (steering input)

    /// EXP-820: while a question or plan card is pending on this steerer
    /// (`cardPending`: live, not ended, an unresolved card in the feed) the
    /// composer band is GONE — the card's free answer is its own inline
    /// field, so the composer would only compete with it. The draft stays in
    /// the model and the band is back once the card resolves. EXP-621: the
    /// composer is tied to the SESSION, not the socket — it stays up through
    /// a reconnect (send disabled, draft intact) and only goes away once the
    /// session is over.
    private func bandRetired(_ model: AgentSessionModel) -> Bool {
        model.isOver || model.cardPending
    }

    @ViewBuilder
    private func bottomBar(_ model: AgentSessionModel) -> some View {
        if bandRetired(model) {
            // EXP-893: the switcher circle alone — the way back to the issue
            // (and to the diff) never leaves the bar.
            FloatingBottomBar {
                EmptyView()
            } center: {
                EmptyView()
            } trailing: {
                switcher()
            }
        } else {
            // Steering is fully seamless (EXP-312) — no captions, no
            // operator state; input just sends.
            VStack(spacing: 8) {
                // EXP-724: the composer's menu rides ABOVE it, inside the
                // same bottom band, so it sits over the feed and above the
                // keyboard instead of being clipped by the field. EXP-802:
                // the `@`/`#`/`:` one is mounted here for the same reason,
                // and `composerMenu` is what keeps them to one at a time.
                composerMenuView(model)
                if composerOpen(model) {
                    composerCard(model)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                } else {
                    collapsedComposerBar(model)
                }
            }
            .animation(motion.standard, value: composerExpanded)
        }
    }

    @ViewBuilder
    private func composerMenuView(_ model: AgentSessionModel) -> some View {
        switch composerMenu(model) {
        case .slash:
            SlashCommandMenu(
                commands: model.slashMatches,
                highlighted: slashHighlight
            ) { command in
                applySlashCommand(command, model)
            }
            .padding(.horizontal, 16)
        case .autocomplete:
            // EXP-802: the same `@`/`#`/`:` menu the comment composer
            // mounts, over the same rows — picks route through the
            // draft editor, which keeps first responder, so the
            // keyboard never drops mid-message.
            EditorAutocompleteMenu(
                mentions: model.draftEditor.mentionCandidates,
                issueRefs: model.draftEditor.issueRefCandidates,
                emoji: model.draftEditor.emojiCandidates,
                selection: model.draftEditor.autocompleteSelection,
                onPickMention: { model.draftEditor.applyMention($0) },
                onPickIssueRef: { model.draftEditor.applyIssueRef($0) },
                onPickEmoji: { model.draftEditor.applyEmoji($0) }
            )
            .padding(.horizontal, 16)
        case .none:
            EmptyView()
        }
    }

    /// EXP-790: open while tapped open, and whenever folding would hide a
    /// draft or a pending image (the model outlives this screen, so a draft
    /// typed before navigating away reopens the field on return).
    private func composerOpen(_ model: AgentSessionModel) -> Bool {
        composerExpanded || !model.trimmedDraft.isEmpty || !model.pendingImages.isEmpty
    }

    /// EXP-893: the folded composer on the shared bar — the usage ring on the
    /// left (the Usage sheet), the capsule wearing the placeholder the open
    /// field would, the face switcher on the right. The separate interrupt
    /// circle is gone: Stop stays the expanded composer's own glyph.
    private func collapsedComposerBar(_ model: AgentSessionModel) -> some View {
        FloatingBottomBar {
            if hasUsage {
                usageRingCircle(model)
            }
        } center: {
            FloatingBarCapsule(accessibilityLabel: "Message the agent", action: expandComposer) {
                Text(model.composerPlaceholder)
                    .font(.subheadline)
                    .lineLimit(1)
            }
            .accessibilityIdentifier("agent-composer-collapsed")
        } trailing: {
            switcher()
        }
    }

    /// 0…1 of the context window, off the engine's latest `usage` slot.
    private func usageFraction(_ model: AgentSessionModel) -> Double? {
        model.sessionUsage?.percent.map { Double($0) / 100 }
    }

    private func usageSeverity(_ model: AgentSessionModel) -> AgentUsageSeverity {
        AgentUsagePresentation.severity(model.sessionUsage?.percent.map(Double.init))
    }

    private func usageRingCircle(_ model: AgentSessionModel) -> some View {
        FloatingBarCircle(accessibilityLabel: "Usage", action: { showUsageSheet = true }) {
            ContextRing(fraction: usageFraction(model), severity: usageSeverity(model))
        }
        .accessibilityIdentifier("session-usage-ring")
    }

    private func expandComposer() {
        withAnimation(motion.standard) { composerExpanded = true }
        // Programmatic focus needs the field mounted — one runloop hop, with
        // a 150ms retry in case the first lands before layout. EXP-802: the
        // same shape as IssueDetailBottomBar's, driven off the editor model.
        DispatchQueue.main.async { focusComposer() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
            if composerExpanded, model?.draftEditor.isEditing == false { focusComposer() }
        }
    }

    /// Hand first responder to the composer's one text block.
    private func focusComposer() {
        guard let editor = model?.draftEditor else { return }
        editor.setFocused(editor.blocks.first?.id)
    }

    // MARK: - Composer menus (EXP-724, EXP-802)

    /// What rides ABOVE the composer in the bottom band. ONE value, because
    /// the band has room for ONE menu.
    ///
    /// The two can never both have content — `SlashCommands.partialName`
    /// rejects whitespace, so a `/` menu needs a draft that is one bare word
    /// starting with `/`, while every `@`/`#`/`:` trigger has to follow
    /// start-of-text or whitespace and carry its own sigil. So this is DEFENCE
    /// against two menus stacking (and against the ↑/↓/Return keys being
    /// claimed twice), not arbitration logic: there is nothing to arbitrate.
    private enum ComposerMenu {
        case none
        case slash
        case autocomplete
    }

    private func composerMenu(_ model: AgentSessionModel) -> ComposerMenu {
        // A folded composer has no field to complete into. Both menus need
        // focus already, but the editor's focus outlives the text view it was
        // handed to, so say it here rather than trust that.
        guard composerOpen(model) else { return .none }
        if slashMenuVisible(model) { return .slash }
        if model.draftEditor.showsAutocompleteMenu { return .autocomplete }
        return .none
    }

    // MARK: - Slash commands (EXP-724)

    /// The menu is up while the field has focus, the draft matches something,
    /// and it hasn't been dismissed at exactly this draft.
    private func slashMenuVisible(_ model: AgentSessionModel) -> Bool {
        model.draftEditor.isEditing && !model.slashMatches.isEmpty
            && slashDismissedFor != model.draftText
    }

    /// Accepting a row REWRITES the draft and never sends — `/name ` when the
    /// command takes an argument, `/name` when it does not. The menu closes on
    /// the accepted draft; typing anything reopens it.
    private func applySlashCommand(_ command: SlashCommand, _ model: AgentSessionModel) {
        model.draftText = command.insertion
        slashDismissedFor = command.insertion
        slashHighlight = 0
        focusComposer()
    }

    /// The row a hardware Return would accept.
    private func highlightedSlashCommand(_ model: AgentSessionModel) -> SlashCommand? {
        let matches = model.slashMatches
        guard matches.indices.contains(slashHighlight) else { return matches.first }
        return matches[slashHighlight]
    }

    /// ↑/↓ wrap around the list (web/Android/desktop parity).
    private func moveSlashHighlight(_ delta: Int, _ model: AgentSessionModel) {
        let count = model.slashMatches.count
        guard count > 0 else { return }
        slashHighlight = ((slashHighlight + delta) % count + count) % count
    }

    // MARK: - Changes face bar (EXP-893)

    /// The PR page a GitHub circle opens — the issue's, or the run's OWN
    /// issue-less one (EXP-734).
    private func prURL(_ model: AgentSessionModel) -> URL? {
        (model.mergeIssue?.prUrl ?? model.session?.prUrl).flatMap { URL(string: $0) }
    }

    /// GitHub · Merge / Fix conflicts · the switcher. Merge only while there
    /// IS an open PR on a session this screen still considers live
    /// (`model.canMerge`); a merge refused on a REAL conflict swaps the pill
    /// for the recovery run (EXP-706).
    private func changesFaceBar(_ model: AgentSessionModel) -> some View {
        FloatingBottomBar {
            // EXP-895: the leading slot is the file list. GitHub keeps the
            // slot only where there is no list to put there — an issue-less
            // run has no header action slot to move it to.
            if !parsedDiff.files.isEmpty {
                DiffFilesBarCircle(count: parsedDiff.files.count) { diffFileSheet = true }
            } else if let url = prURL(model) {
                FloatingBarCircle(
                    accessibilityLabel: DomainContract.diffUiOpenOnGithub,
                    action: { openURL(url) }
                ) {
                    AppIcon(AppIcons.uiGithub, size: AppIcon.Size.medium, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
        } center: {
            if model.canMerge {
                if canFixConflicts {
                    fixConflictsPill()
                } else {
                    mergePill(model)
                }
            }
        } trailing: {
            switcher()
        }
    }

    /// The Merge pill — merging always ends the run too (EXP-498).
    private func mergePill(_ model: AgentSessionModel) -> some View {
        FloatingBarSolidPill(
            accessibilityLabel: "Merge pull request",
            enabled: !merging,
            action: { showMergeConfirm = true }
        ) {
            if merging {
                ProgressView().controlSize(.small).tint(.black.opacity(0.6))
            } else {
                AppIcon(AppIcons.prMerged, size: AppIcon.Size.medium, weight: .medium)
            }
            Text(DomainContract.diffUiMergePr)
                .font(.subheadline.weight(.medium))
        }
    }

    /// EXP-706: the recovery run in the Merge pill's slot. EXP-825: it pushes
    /// the Agent page composer with the "Fix merge conflicts" builtin picked
    /// and THIS run's pull request pre-picked.
    private func fixConflictsPill() -> some View {
        FloatingBarSolidPill(accessibilityLabel: "Fix merge conflicts", action: openFixConflicts) {
            AppIcon(AppIcons.uiBranch, size: AppIcon.Size.medium, weight: .medium)
            Text("Fix conflicts")
                .font(.subheadline.weight(.medium))
        }
    }

    private func openFixConflicts() {
        guard let issueId = model?.mergeIssue?.id else { return }
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(
                actionId: DomainContract.builtinFixConflictsId,
                prIssueId: issueId
            )
        ))
    }

    /// Only a REAL content conflict (EXP-533) gets the run — every other
    /// refusal (stale head, branch protection, a misconfigured GitHub App) is
    /// something no rebase can fix. The run rebases the PR's branch, so one
    /// must be recorded on the issue behind the merge. EXP-734: the builtin
    /// takes an ISSUE-linked PR, so a run's own issue-less PR gets no recovery
    /// offer (the caption still names the refusal).
    private var canFixConflicts: Bool {
        guard case .issue = model?.mergeTarget else { return false }
        return steerEnabled
            && mergeFailure?.isConflict == true
            && !(model?.mergeIssue?.branch ?? "").isEmpty
    }

    /// Merge the session's PR. No local surgery on success: the server ends
    /// the session and flips `pr_state`, and both land here through sync.
    /// EXP-734: an action or chat run's PR links no issue, so it merges
    /// through the session row the server stamped it on.
    private func merge(_ model: AgentSessionModel) {
        guard let target = model.mergeTarget else { return }
        mergeFailure = nil
        merging = true
        Task {
            do {
                switch target {
                case let .issue(issueId):
                    try await deps.issuesApi.mergePr(accountId: accountId, issueId: issueId)
                case let .session(sessionId):
                    try await deps.codingSessionsApi.mergePr(
                        accountId: accountId, sessionId: sessionId
                    )
                }
            } catch {
                mergeFailure = MergeFailure(error: error)
            }
            merging = false
        }
    }

    // MARK: - Composer card

    /// EXP-554: the steer composer wears the comment composer's chrome, and
    /// since EXP-698 that IS the same object — one `GlassComposer` holding the
    /// transparent field, the pending strip and the footer row. EXP-893: the
    /// footer is `SessionComposerFooter` (Plan mode · `+` · model · ring).
    private func composerCard(_ model: AgentSessionModel) -> some View {
        // EXP-702: images attach to the SESSION, so every run can carry them —
        // a batch or action run no longer has "nowhere to put them".
        let attachFull = model.pendingImages.count >= SteerImageMessage.maxImages
        let canSend = !model.trimmedDraft.isEmpty || !model.pendingImages.isEmpty
        // EXP-621: only SENDING waits for the socket — the field, the picker
        // and the strip stay usable so the draft is ready the moment the
        // reconnect lands.
        let sendDisabled = !canSend || model.steerSending || !model.canSteer
        let attachDisabled = attachFull || model.steerSending
        // EXP-790: an EMPTY field while the agent works offers Stop (the
        // interrupt) in the send slot; the first typed character brings Send
        // back.
        let showsStop = model.agentWorking && !canSend
        return GlassComposer(isOpaque: true) {
            // EXP-802: a caret-bearing field, so `@` members, `#` issues and
            // `:` emoji complete here exactly as they do in a comment. The
            // return key SENDS (the field is one message, never a document),
            // which is also how Return accepts an open menu below.
            MarkdownComposerField(
                model: model.draftEditor,
                // EXP-820: the generic prompt — a pending card's free answer
                // is the card's own inline field, and this band hides while
                // one is pending.
                placeholder: model.composerPlaceholder,
                onReturn: { handleComposerReturn(model) },
                onPasteImage: { image in ingestPastedImage(model, image) },
                onIssueRefTap: { issueId in
                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                },
                // One line of body text plus the field's own 12pt inset,
                // growing to about five before it scrolls inside itself.
                minHeight: 34,
                maxHeight: 120
            )
            // EXP-724: hardware-keyboard driving of the `/` menu. Every
            // handler returns `.ignored` while no menu is open, so a
            // Bluetooth keyboard behaves exactly as it did before. (Return is
            // NOT here: the text view intercepts it itself, so it works for
            // the soft keyboard too — see `handleComposerReturn`.)
            .onKeyPress(.upArrow) {
                guard composerMenu(model) == .slash else { return .ignored }
                moveSlashHighlight(-1, model)
                return .handled
            }
            .onKeyPress(.downArrow) {
                guard composerMenu(model) == .slash else { return .ignored }
                moveSlashHighlight(1, model)
                return .handled
            }
            .onKeyPress(.escape) {
                guard composerMenu(model) == .slash else { return .ignored }
                slashDismissedFor = model.draftText
                return .handled
            }
            .onChange(of: model.draftText) { _, _ in
                slashHighlight = 0
            }
            // The text view carries a 6pt inset of its own (`singleLine`), so
            // the card's 12/12/6 band is spelled 6/6/0 here.
            .padding(.horizontal, 6)
            .padding(.top, 6)
        } strip: {
            if !model.pendingImages.isEmpty {
                PendingAttachmentStrip(items: model.pendingImages) { id in
                    removePendingImage(model, id: id)
                }
                .padding(.horizontal, 8)
                .padding(.bottom, 4)
            }

            if let imageError = model.steerImageError {
                Text(imageError)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 4)
            }
        } tools: {
            SessionComposerFooter(
                planModeActive: model.planModeActive,
                attachEnabled: !attachDisabled,
                onAttach: { showPhotoPicker = true },
                modelValue: WorkFaces.sessionModel(model.sessionConfig),
                agent: model.catalogAgent,
                onPickModel: { alias in sendModelSwitch(model, alias) },
                usageFraction: usageFraction(model),
                usageSeverity: usageSeverity(model),
                onUsage: { showUsageSheet = true }
            )
        } submit: {
            GlassComposerSubmitButton(
                showsStop ? AppIcons.uiStop : AppIcons.uiSubmit,
                accessibilityLabel: showsStop ? "Stop" : "Send",
                enabled: showsStop ? model.canSteer : !sendDisabled
            ) {
                if showsStop {
                    model.sendInterrupt()
                } else {
                    sendMessage(model)
                }
            }
        }
    }

    /// EXP-877: a model switch is a PLAIN MESSAGE — `/model <alias>` — the
    /// agent answers with the next `config_state`, which repaints the pill.
    private func sendModelSwitch(_ model: AgentSessionModel, _ alias: String) {
        _ = model.sendMessage("/model \(alias)")
    }

    /// The return key. With a menu open it ACCEPTS the top row and never
    /// sends — the `/` menu's highlighted command (↑/↓ move it), or the
    /// candidate the `@`/`#`/`:` list is showing first, which is the row every
    /// other client's Enter takes.
    private func handleComposerReturn(_ model: AgentSessionModel) {
        switch composerMenu(model) {
        case .slash:
            guard let command = highlightedSlashCommand(model) else { return }
            applySlashCommand(command, model)
        case .autocomplete:
            let editor = model.draftEditor
            if let member = editor.mentionCandidates.first {
                editor.applyMention(member)
            } else if let candidate = editor.issueRefCandidates.first {
                editor.applyIssueRef(candidate)
            } else if let record = editor.emojiCandidates.first {
                editor.applyEmoji(record)
            }
        case .none:
            sendMessage(model)
        }
    }

    private func sendMessage(_ model: AgentSessionModel) {
        // EXP-724: `/clear` discards the whole conversation — ask
        // first, then send exactly what was typed.
        if let command = model.pendingSlashCommand, command.confirm {
            slashConfirm = command
            return
        }
        performSend(model)
    }

    private func performSend(_ model: AgentSessionModel) {
        guard !model.pendingImages.isEmpty else {
            guard !model.trimmedDraft.isEmpty else { return }
            // Clear ONLY once the frames are actually out (EXP-621): a send
            // into a dropped socket used to wipe the composer with nothing
            // sent.
            if model.sendMessage(model.draftText) { model.draftText = "" }
            return
        }
        guard !model.steerSending else { return }
        Task { await sendWithImages(model) }
    }

    /// Upload the pending images and send ONE composed message (EXP-511). A
    /// failure keeps the draft and the strip — the model hands back the images
    /// with their already-uploaded ids so a retry only uploads the rest.
    private func sendWithImages(_ model: AgentSessionModel) async {
        if let remaining = await model.sendSteerImages(
            model.draftText, images: model.pendingImages
        ) {
            model.pendingImages = remaining
        } else {
            model.draftText = ""
            model.pendingImages = []
        }
    }

    /// Turn a photo pick into pending images: transcode anything the server's
    /// inline-image pipeline doesn't accept (notably HEIC) to JPEG, exactly as
    /// the share extension does, and cap the strip at four.
    private func ingestPhotos(_ items: [PhotosPickerItem]) async {
        defer { photoItems = [] }
        guard let model else { return }
        model.steerImageError = nil
        for item in items {
            guard model.pendingImages.count < SteerImageMessage.maxImages else { break }
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let type = item.supportedContentTypes.first
            // EXP-554: one shared normalizer for every composer (steer + the two
            // comment ones) — transcode, canonical type, 10 MB cap.
            let outcome = AttachmentPicks.normalizedPhoto(
                data: data,
                contentTypeHint: type?.preferredMIMEType,
                filenameExtensionHint: type?.preferredFilenameExtension
            )
            guard let normalized = outcome.attachment else {
                model.steerImageError = outcome.failure
                continue
            }
            queuePendingImage(model, normalized)
        }
    }

    /// EXP-802: a PASTED image joins the strip like a picked one — the field
    /// is a text view now, so an image paste actually reaches the composer. It
    /// can never become an image BLOCK: the draft is exactly one text block.
    private func ingestPastedImage(_ model: AgentSessionModel, _ image: UIImage) {
        guard model.pendingImages.count < SteerImageMessage.maxImages else { return }
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        model.steerImageError = nil
        let outcome = AttachmentPicks.normalizedPhoto(
            data: data, contentTypeHint: "image/jpeg", filenameExtensionHint: "jpg"
        )
        guard let normalized = outcome.attachment else {
            model.steerImageError = outcome.failure
            return
        }
        queuePendingImage(model, normalized)
    }

    /// Queue one normalized image and drop its positional `[Image #k]` marker
    /// into the draft, so a sentence can name the picture it means (EXP-698).
    ///
    /// The marker lands at the END of the draft, not at the caret: the draft
    /// now HAS a caret (EXP-802), but it is an offset into the DECORATED text,
    /// where a resolved `#EXP-1` chip carries a title character that the sent
    /// text does not. Mapping it back is not worth it for a token that names
    /// the k-th of at most four images.
    private func queuePendingImage(
        _ model: AgentSessionModel, _ normalized: PendingCommentAttachment
    ) {
        model.pendingImages.append(PendingSteerImage(
            data: normalized.data,
            filename: normalized.filename,
            contentType: normalized.contentType,
            uploadedId: nil
        ))
        let inserted = SteerImageMessage.insertImageMarker(
            text: model.draftText,
            caret: (model.draftText as NSString).length,
            index: model.pendingImages.count
        )
        model.draftText = inserted.text
    }

    /// Dropping a pending image renumbers the draft's markers: the removed
    /// one goes and every higher one slides down, so `[Image #N]` keeps
    /// naming the N-th image that will actually be sent.
    private func removePendingImage(_ model: AgentSessionModel, id: UUID) {
        guard let position = model.pendingImages.firstIndex(where: { $0.id == id }) else { return }
        model.pendingImages.remove(at: position)
        model.draftText = SteerImageMessage.renumberImageMarkers(
            model.draftText, removedIndex: position + 1
        )
    }
}

// MARK: - Feed rows

/// EXP-787 — the transcript's own type scale, straight off the shared tokens:
/// prose at `bodySize` on a `bodyLineHeight` line, every tool row and caption
/// at `toolSize`/`toolLineHeight`.
///
/// Those numbers are the DEFAULT-size values, not fixed points: each one goes
/// through `UIFontMetrics` for the text style it stands in for (prose `.body`,
/// tool rows `.caption1`), so the transcript keeps Dynamic Type exactly as the
/// `.subheadline`/`.caption` semantics it replaced did. `lineSpacing` is
/// LEADING and SwiftUI has no line-height modifier, so it is the difference
/// between the SCALED line box and the SCALED font — resolved per render,
/// never precomputed. `AgentMarkdownText.chatCodePalette` carries the same two
/// tokens into the markdown renderer, which resolves them the same way.
enum TranscriptType {
    static func bodyFont() -> Font { Font(bodyUIFont()) }

    static func bodyLineSpacing() -> CGFloat {
        leading(
            lineHeight: DesignTokens.Transcript.bodyLineHeight,
            font: bodyUIFont(),
            textStyle: .body
        )
    }

    static func toolFont(_ weight: PlatformFont.Weight = .regular) -> Font {
        Font(toolUIFont(weight))
    }

    static func toolLineSpacing() -> CGFloat {
        leading(
            lineHeight: DesignTokens.Transcript.toolLineHeight,
            font: toolUIFont(.regular),
            textStyle: .caption1
        )
    }

    private static func bodyUIFont() -> PlatformFont {
        scaled(size: DesignTokens.Transcript.bodySize, weight: .regular, textStyle: .body)
    }

    private static func toolUIFont(_ weight: PlatformFont.Weight) -> PlatformFont {
        scaled(size: DesignTokens.Transcript.toolSize, weight: weight, textStyle: .caption1)
    }

    private static func scaled(
        size: CGFloat, weight: PlatformFont.Weight, textStyle: PlatformFont.TextStyle
    ) -> PlatformFont {
        UIFontMetrics(forTextStyle: textStyle)
            .scaledFont(for: .systemFont(ofSize: size, weight: weight))
    }

    private static func leading(
        lineHeight: CGFloat, font: PlatformFont, textStyle: PlatformFont.TextStyle
    ) -> CGFloat {
        max(0, UIFontMetrics(forTextStyle: textStyle).scaledValue(for: lineHeight) - font.lineHeight)
    }
}

/// A tool row / transcript caption: 12pt on an 18pt line at the default text
/// size, scaled from there.
struct TranscriptToolText: ViewModifier {
    let weight: PlatformFont.Weight
    /// `UIFontMetrics` resolves off the CURRENT trait collection, which SwiftUI
    /// cannot see into — reading the size category is what makes this view
    /// re-evaluate (and re-bake the font) when the reader changes it.
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    func body(content: Content) -> some View {
        let _ = dynamicTypeSize
        content
            .font(TranscriptType.toolFont(weight))
            .lineSpacing(TranscriptType.toolLineSpacing())
    }
}

extension View {
    func transcriptToolText(_ weight: PlatformFont.Weight = .regular) -> some View {
        modifier(TranscriptToolText(weight: weight))
    }
}

/// Assistant prose — a small glyph + a full-width selectable markdown render
/// (EXP-440: claude narrates in markdown, so lists, code and images have to
/// come out as themselves). EXP-274 dropped the glass speech bubble: agent
/// output is the feed's bulk, and the bubble insets cost real width on a phone.
private struct NarrationBubble: View {
    let text: String
    let context: AgentMarkdownContext

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.codingAssistant, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.top, 4)
            // EXP-440: narration is markdown (lists, code, images), rendered
            // through the block stack. Bare URLs stay tappable — the remote
            // `/login` flow publishes the claude sign-in URL as narration
            // (EXP-430) — now via cmark's GFM autolink rather than the hand
            // tokenizer this replaced.
            AgentMarkdownText(
                text: text,
                context: context,
                options: [.autolinkBareURLs, .hardLineBreaks]
            )
        }
    }
}

/// A human turn (EXP-78): the initial prompt or a steered message — rendered
/// trailing-aligned like the sender's own chat bubble, long text folded.
private struct UserMessageBubble: View {
    let text: String
    let context: AgentMarkdownContext

    @State private var expanded = false

    /// Fold threshold — the initial prompt can be 16 KiB.
    private static let clampLines = 6
    private static let clampChars = 600
    /// Folded height. A block render has no `lineLimit`, so the fold is a
    /// clipped height cap instead — same approach as QuestionCard's prompt.
    private static let clampHeight: CGFloat = 160

    private func isClampable(_ body: String) -> Bool {
        body.count > Self.clampChars
            || body.filter { $0 == "\n" }.count >= Self.clampLines
    }

    /// EXP-698: a steered message with images (EXP-511) splits into its PROSE
    /// — whose `[Image #N]` markers become chips — and the embeds themselves,
    /// which stay below it. A message without embeds is the wire message and
    /// keeps the full markdown block render, markers and all: a marker with no
    /// picture behind it is prose, and chipping it would promise a preview
    /// that does not exist (web's `MarkerText` `count` gate).
    private var parsed: SteerImageMessage.Parsed { SteerImageMessage.parse(text) }

    var body: some View {
        let parsed = parsed
        let hasImages = !parsed.attachmentIds.isEmpty
        // Clamp the PROSE, never the wire message: four embed lines are four
        // more "lines" of nothing, and they used to push a two-line steer into
        // the fold — which then hid the images the fold was measuring.
        let clampable = isClampable(hasImages ? parsed.text : text)
        return HStack {
            Spacer(minLength: 32)
            VStack(alignment: .leading, spacing: 4) {
                Group {
                    if !hasImages {
                        AgentMarkdownText(
                            text: text,
                            context: context,
                            options: [.autolinkBareURLs, .hardLineBreaks],
                            hugsWidth: true
                        )
                    } else if !parsed.text.isEmpty {
                        if parsed.markers.isEmpty {
                            AgentMarkdownText(
                                text: parsed.text,
                                context: context,
                                options: [.autolinkBareURLs, .hardLineBreaks],
                                hugsWidth: true
                            )
                        } else {
                            MarkedUpUserText(
                                text: parsed.text,
                                imageCount: parsed.attachmentIds.count
                            )
                        }
                    }
                }
                .frame(maxHeight: clampable && !expanded ? Self.clampHeight : nil, alignment: .top)
                .clipped()
                if clampable {
                    Button(expanded ? "Show less" : "Show more") {
                        expanded.toggle()
                    }
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .buttonStyle(.plain)
                }
                // OUTSIDE the fold: the images are the point of the message,
                // and a long prose clamp must never be what hides them.
                if hasImages {
                    AgentMarkdownText(
                        text: SteerImageMessage.build(
                            text: "", attachmentIds: parsed.attachmentIds
                        ),
                        context: context,
                        options: [.autolinkBareURLs, .hardLineBreaks],
                        hugsWidth: true
                    )
                    .padding(.top, parsed.text.isEmpty ? 0 : 4)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            // Slightly brighter than the assistant's glass sections — the
            // sender's own bubble (matches the composer's active tint).
            .background(Color.white.opacity(0.10))
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .stroke(Color.white.opacity(0.16), lineWidth: 0.5)
            )
        }
    }
}

/// EXP-698: the PROSE of a steered message that carries `[Image #N]` markers.
/// The words flow as plain runs and a marker becomes a readonly `GlassPill`
/// beside them, so "crop [Image #2] tighter" reads as a sentence with a chip
/// in it. Markdown is deliberately NOT re-parsed here — a message that names
/// its pictures came from the composer as chat prose, and a block stack could
/// not flow around an inline chip anyway — but the two things the plain path
/// would otherwise lose are kept: line breaks (one flow row per line) and
/// bare URLs (rendered as `Link`, what cmark's autolink does on the markdown
/// path). The embeds are the caller's; this view renders text only.
private struct MarkedUpUserText: View {
    let text: String
    /// How many images the message actually carries. A marker outside
    /// `1...imageCount` — hand-typed, or left behind by an edit — is prose,
    /// not a chip: chipping it would promise a picture that is not there.
    let imageCount: Int
    /// EXP-787: this row bakes its own `UIFont` off `UIFontMetrics`, so it has
    /// to re-evaluate when the reader's text size changes.
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    /// One flowed item. Words are split out so the flow layout can wrap
    /// between them — a whole paragraph as one subview would simply overflow
    /// the bubble.
    private enum Piece: Identifiable {
        /// `text` is the word; when `url` is set only `text` is the anchor and
        /// `suffix` is the prose punctuation that followed it inside the same
        /// whitespace-delimited word (`https://x.dev).` → anchor + `).`).
        case word(id: Int, text: String, url: URL?, suffix: String)
        case marker(id: Int, index: Int)

        var id: Int {
            switch self {
            case .word(let id, _, _, _), .marker(let id, _): return id
            }
        }
    }

    /// A source line's pieces. Splitting on `\n` FIRST is what keeps a
    /// multi-line steer multi-line — one flow row per line, instead of every
    /// word of the message poured into a single wrapping paragraph.
    private struct Line: Identifiable {
        let id: Int
        let pieces: [Piece]
    }

    private var lines: [Line] {
        var lines: [Line] = []
        var current: [Piece] = []
        var nextId = 0

        func take() -> Int {
            defer { nextId += 1 }
            return nextId
        }
        func breakLine() {
            lines.append(Line(id: lines.count, pieces: current))
            current = []
        }

        for segment in SteerImageMessage.segments(of: text) {
            switch segment {
            case .text(let run):
                let runLines = run.components(separatedBy: "\n")
                for (offset, runLine) in runLines.enumerated() {
                    if offset > 0 { breakLine() }
                    for word in runLine.split(whereSeparator: \.isWhitespace) {
                        let word = String(word)
                        let anchor = Self.link(in: word)
                        current.append(.word(
                            id: take(),
                            text: anchor?.text ?? word,
                            url: anchor?.url,
                            suffix: anchor?.suffix ?? ""
                        ))
                    }
                }
            case .marker(let index):
                if index >= 1 && index <= imageCount {
                    current.append(.marker(id: take(), index: index))
                } else {
                    // Out of range: prose, exactly as the wire carried it.
                    current.append(.word(
                        id: take(),
                        text: SteerImageMessage.imageMarker(index),
                        url: nil,
                        suffix: ""
                    ))
                }
            }
        }
        breakLine()
        return lines
    }

    var body: some View {
        let _ = dynamicTypeSize
        // EXP-787: the flow rows ARE the message's lines, so their spacing is
        // the transcript's leading — a `lineSpacing` on the per-word `Text`s
        // could never reach them.
        VStack(alignment: .leading, spacing: TranscriptType.bodyLineSpacing()) {
            ForEach(lines) { line in
                if line.pieces.isEmpty {
                    // A blank line keeps its height — the paragraph break the
                    // sender typed is part of the message.
                    Text(verbatim: " ")
                        .font(TranscriptType.bodyFont())
                        .foregroundStyle(.clear)
                } else {
                    FlowLayout(spacing: 4) {
                        ForEach(line.pieces) { piece in
                            switch piece {
                            case .word(_, let word, let url, let suffix):
                                if let url {
                                    // One subview, no flow spacing inside it:
                                    // the punctuation belongs against the
                                    // anchor, not a space away from it.
                                    HStack(spacing: 0) {
                                        Link(destination: url) {
                                            Text(word)
                                                .font(TranscriptType.bodyFont())
                                                .underline()
                                                .foregroundStyle(Color(MarkdownStyle.linkColor))
                                        }
                                        if !suffix.isEmpty {
                                            Text(suffix)
                                                .font(TranscriptType.bodyFont())
                                                .foregroundStyle(.white.opacity(0.9))
                                        }
                                    }
                                } else {
                                    Text(word)
                                        .font(TranscriptType.bodyFont())
                                        .foregroundStyle(.white.opacity(0.9))
                                }
                            case .marker(_, let index):
                                GlassPill("Image \(index)", icon: AppIcons.editorImage)
                            }
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A bare `http(s)://` word, with prose punctuation trimmed off the end —
    /// `(see https://x.dev).` must not link the `).`. Balanced closing
    /// brackets are kept, so `https://x.dev/a(b)` stays whole. Hand-mirrored
    /// from web's `lib/linkify.ts`, scoped to one whitespace-delimited word
    /// because that is all this flow layout ever hands it.
    private static func link(in word: String) -> (text: String, url: URL, suffix: String)? {
        let lowered = word.lowercased()
        guard lowered.hasPrefix("http://") || lowered.hasPrefix("https://") else { return nil }
        var anchor = Substring(word)
        while let last = anchor.last {
            let opens: (Character) -> Int = { open in anchor.filter { $0 == open }.count }
            if ".,;:!?".contains(last) {
                anchor = anchor.dropLast()
            } else if last == ")", opens("(") < opens(")") {
                anchor = anchor.dropLast()
            } else if last == "]", opens("[") < opens("]") {
                anchor = anchor.dropLast()
            } else {
                break
            }
        }
        guard !anchor.isEmpty, let url = URL(string: String(anchor)) else { return nil }
        return (String(anchor), url, String(word.dropFirst(anchor.count)))
    }
}

/// An interactive question (EXP-78): AskUserQuestion step or plan approval.
///
/// Every card carries a wire id (EXP-613), so a tap sends ONE semantic
/// `answer` frame (the desktop maps keys to its own picker and confirms with
/// `answer_ack`) and the card locks the moment it goes out — a second tap used
/// to land on the NEXT question. There is no other answer path: EXP-672 retired
/// the pre-EXP-249 raw-keystroke fallback. `planMode` cards (EXP-97) get a
/// dedicated "Plan ready" presentation with the first wire option as the
/// primary approve action; prompt and plan bodies render as markdown, which is
/// what claude writes.
private struct QuestionCard: View {
    let question: AgentQuestion
    /// "Question 2 of 3" for a step of a multi-question ask; nil for a lone card.
    var stepLabel: String? = nil
    /// The ask's already-answered steps, summarized above this one.
    var priorSteps: [AgentQuestion] = []
    /// Per prior step: its answer — the desktop-resolved one, else what this
    /// client picked (EXP-588); nil = answered elsewhere, unknown here.
    var priorAnswers: [String?] = []
    /// What this client picked for THIS card — shown once it resolves when
    /// the desktop's resolution carried no answer text (EXP-588).
    var localAnswer: String? = nil
    /// Still answerable per the feed — the session is blocked on this card.
    let active: Bool
    /// Live (and not ended) — whether this client may answer at all.
    let canAnswer: Bool
    /// An answer is already out (or confirmed) for this card — every control
    /// stays dead until the desktop resolves it or the lock expires.
    let locked: Bool
    /// Locked but not yet confirmed by the desktop (`answer_ack`).
    let pending: Bool
    /// The last answer expired unconfirmed — answerable again, with a retry
    /// hint so the rollback isn't a silent mystery (EXP-334, web parity).
    let failed: Bool
    /// EXP-820: the ask this card belongs to is still OPEN
    /// (`AgentFeed.askComplete` false) — gates the lock captions and the
    /// answered rows' edit affordance. A lone card is its own ask.
    var askOpen: Bool = true
    /// EXP-820: this is an EARLIER step being re-answered ("go back") — the
    /// resolution branch is skipped and the options are live although the
    /// step already resolved or locked.
    var editing: Bool = false
    /// EXP-820: the recorded answer to pre-select while editing.
    var editingAnswer: String? = nil
    /// EXP-820: while editing, the ask's CURRENT step folded into a muted
    /// row — tapping it returns to it.
    var foldedCurrent: AgentQuestion? = nil
    /// EXP-820: an answered step row was tapped (its wire id); nil = the rows
    /// are not tappable (the ask is over, or this client cannot answer).
    var onEditStep: ((String) -> Void)? = nil
    /// EXP-820: leave editing without answering.
    var onExitEditing: (() -> Void)? = nil
    /// Protocol v2: one semantic frame carrying every chosen key; the second
    /// argument is the typed reply for a `freeText` option (EXP-513).
    let onAnswer: ([String], String?) -> Void
    /// EXP-820: a plan's inline feedback — answer on `keys` (the reject row),
    /// THEN send the text as the next ordinary message.
    var onAnswerThenSend: (([String], String) -> Void)? = nil
    /// Image fetching for the prompt's markdown render (EXP-440).
    var markdownContext: AgentMarkdownContext? = nil

    @State private var expanded = false
    /// Tap order is the submit order of the semantic answer frame.
    @State private var picked: [String] = []
    /// EXP-820: the option whose inline field is open — a free-text row
    /// ("Type something.") or the plan's reject row. Tapping the row again
    /// folds it.
    @State private var inlineKey: String? = nil
    @State private var inlineText = ""
    @FocusState private var inlineFocused: Bool

    private static let clampChars = 600
    private static let clampLines = 6
    private static let clampHeight: CGFloat = 160
    /// EXP-788: how many rows get a numbered chip — the desktop's keystrokes
    /// are single digits.
    private static let numberedRows = 9

    /// Plans are always fully rendered — never folded (EXP-197).
    private var clampable: Bool {
        !question.planMode
            && (question.text.count > Self.clampChars
                || question.text.filter { $0 == "\n" }.count >= Self.clampLines)
    }

    /// EXP-820: an edited step answers again past its resolution.
    private var answerable: Bool { editing || (active && canAnswer) }

    private var showsResolution: Bool { question.resolved && !editing }

    /// EXP-820: answered rows are tappable only while the ask is open and this
    /// client may answer — the host hands a handler in exactly then.
    private var canEditPriorSteps: Bool { askOpen && canAnswer && onEditStep != nil }

    /// EXP-820: the plan's reject row — the one option that expands into the
    /// inline feedback field (`AgentFeed.planRejectKey` rule, ×4).
    private var rejectKey: String? { AgentFeed.planRejectKey(for: question) }

    /// EXP-820: a row that expands into an inline field instead of answering
    /// on the tap — a free-text row, or the plan's reject row.
    private func expandsInline(_ option: AgentQuestionOption) -> Bool {
        option.freeText || option.key == rejectKey
    }

    private var headerText: String? {
        if question.planMode { return "Plan ready" }
        var parts: [String] = []
        if let stepLabel {
            parts.append(stepLabel)
        } else if question.isSubmitStep {
            parts.append("Review")
        }
        if let header = question.header { parts.append(header) }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// Multi-select answers batch into one submit; everything else answers on
    /// the option tap.
    private var needsExplicitSubmit: Bool { question.multiSelect }

    private var submitTitle: String { AgentFeed.submitLabel }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            // EXP-820: no blue on the card — a plan's glyph and header are the
            // primary white, a question's stay the semantic yellow.
            AppIcon(question.planMode ? AppIcons.codingPlan : AppIcons.uiHelp, size: AppIcon.Size.small)
                .foregroundStyle(
                    question.planMode ? Color.white : DesignTokens.Semantic.yellow
                )
                .padding(.top, 4)
            VStack(alignment: .leading, spacing: 8) {
                if let headerText {
                    Text(headerText)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(
                            question.planMode
                                ? Color.white
                                : Color.white.opacity(TextOpacity.secondary)
                        )
                }
                priorStepSummary
                prompt
                if showsResolution {
                    resolution
                } else {
                    optionList
                    trailingActions
                }
                if editing {
                    editingFooter
                }
                if active, !canAnswer {
                    Text(question.planMode
                        ? "Waiting for approval. You're viewing read-only."
                        : "Waiting for an answer. You're viewing read-only.")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        // A bordered card, not a group container: the ask is one free-content
        // block that has to stand off the transcript behind it.
        .glassCard()
        .onAppear(perform: preselectEditingAnswer)
    }

    /// EXP-820: an edited step starts from its recorded answer — the row whose
    /// label matches is picked (every matching row on a multi-select, whose
    /// summary is the labels joined by ", "); a typed free-text answer with no
    /// matching row reopens the free-text field with the text in it.
    private func preselectEditingAnswer() {
        guard editing, picked.isEmpty, inlineKey == nil, let editingAnswer else { return }
        if let exact = question.options.first(where: { $0.label == editingAnswer }) {
            picked = [exact.key]
            return
        }
        if question.multiSelect {
            let parts = editingAnswer.components(separatedBy: ", ")
            let matches = question.options.filter { parts.contains($0.label) }.map(\.key)
            if !matches.isEmpty {
                picked = matches
                return
            }
        }
        if let free = question.options.first(where: { $0.freeText }) {
            inlineKey = free.key
            inlineText = editingAnswer
        }
    }

    /// The step's one-line title: its header when it has one, else its text.
    private func stepTitle(_ step: AgentQuestion) -> String {
        if let header = step.header, !header.isEmpty { return header }
        return step.text
    }

    /// The answered steps of this ask, so the stepper still shows what was
    /// asked and what was chosen — the question folded to one line next to
    /// its answer (web `AnsweredStepRow` parity, EXP-588). EXP-820: while the
    /// ask is open each row is a button that swaps that step back in.
    @ViewBuilder
    private var priorStepSummary: some View {
        if !priorSteps.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(priorSteps.enumerated()), id: \.element.id) { position, step in
                    let answer = position < priorAnswers.count ? priorAnswers[position] : nil
                    if canEditPriorSteps {
                        Button {
                            onEditStep?(step.wireId)
                        } label: {
                            priorStepRow(step, answer: answer, editable: true)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Change the answer to \(stepTitle(step))")
                    } else {
                        priorStepRow(step, answer: answer, editable: false)
                    }
                }
            }
        }
    }

    private func priorStepRow(_ step: AgentQuestion, answer: String?, editable: Bool) -> some View {
        HStack(alignment: .top, spacing: 6) {
            AppIcon(step.dismissed ? AppIcons.uiClose : AppIcons.uiCheck, size: 11)
                .foregroundStyle(
                    step.dismissed
                        ? Color.white.opacity(TextOpacity.tertiary)
                        : DesignTokens.Semantic.green
                )
                .padding(.top, 2)
            Text(stepTitle(step))
                .font(.caption)
                .lineLimit(1)
                .truncationMode(.tail)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(answer ?? (step.dismissed ? "Dismissed" : "Answered"))
                .font(.caption.weight(.medium))
                .lineLimit(2)
                .multilineTextAlignment(.trailing)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(maxWidth: 180, alignment: .trailing)
                .fixedSize(horizontal: false, vertical: true)
            if editable {
                // EXP-820: the "go back" cue — the row re-opens this step.
                AppIcon(AppIcons.uiEdit, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.top, 2)
            }
        }
    }

    /// EXP-820: under an edited step's options — the current step folded into
    /// a muted row, and the text button back to it. Both leave editing.
    @ViewBuilder
    private var editingFooter: some View {
        if let foldedCurrent {
            Button {
                onExitEditing?()
            } label: {
                HStack(alignment: .top, spacing: 6) {
                    AppIcon(AppIcons.uiChevronRight, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.top, 2)
                    Text(stepTitle(foldedCurrent))
                        .font(.caption)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(AgentFeed.backToCurrentStepLabel)
        }
        Button(AgentFeed.backToCurrentStepLabel) {
            onExitEditing?()
        }
        .font(.caption2.weight(.medium))
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        .buttonStyle(.plain)
        .accessibilityIdentifier("agent-question-back-to-current")
    }

    @ViewBuilder
    private var prompt: some View {
        let clamped = clampable && !expanded
        // No hard line breaks: a prompt or plan body is authored GFM prose,
        // where a wrapped source line is one paragraph.
        AgentMarkdownText(
            text: question.text,
            context: markdownContext,
            options: [.autolinkBareURLs]
        )
        .frame(maxHeight: clamped ? Self.clampHeight : nil, alignment: .top)
        .clipped()
        if clampable {
            Button(expanded ? "Show less" : "Show more") {
                expanded.toggle()
            }
            .font(.caption2.weight(.medium))
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .buttonStyle(.plain)
        }
    }

    @ViewBuilder
    private var resolution: some View {
        HStack(alignment: .top, spacing: 6) {
            AppIcon(question.dismissed ? AppIcons.uiClose : AppIcons.uiCheck, size: 11)
                .foregroundStyle(
                    question.dismissed
                        ? Color.white.opacity(TextOpacity.tertiary)
                        : DesignTokens.Semantic.green
                )
                .padding(.top, 2)
            Text(
                question.answerSummary
                    ?? (question.dismissed ? nil : localAnswer)
                    ?? (question.dismissed ? "Dismissed" : "Answered")
            )
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
                .textSelection(.enabled)
        }
    }

    /// EXP-788: every option is a real full-width button — a numbered chip
    /// (1..9, the desktop's keystroke) or, on a multi-select, its checkbox,
    /// then the label with the option's description under it. The wire's
    /// first option of a plan (and of an ask's review step, "Submit answers")
    /// is the primary action — the contract puts it first — and wears the
    /// solid primary fill. EXP-820: a free-text row and the plan's reject row
    /// are drawn too and EXPAND into an inline field under themselves.
    @ViewBuilder
    private var optionList: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(question.options.enumerated()), id: \.element.key) { index, option in
                let primary = (question.planMode || question.isSubmitStep) && index == 0
                let selected = picked.contains(option.key)
                let inlineOpen = inlineKey == option.key
                VStack(alignment: .leading, spacing: 6) {
                    if answerable {
                        Button {
                            pick(option)
                        } label: {
                            optionRow(
                                option,
                                number: index + 1,
                                primary: primary,
                                checked: question.multiSelect ? selected : nil,
                                selected: selected,
                                expandable: expandsInline(option),
                                inlineOpen: inlineOpen
                            )
                        }
                        .buttonStyle(.plain)
                        .disabled(locked)
                        .opacity(locked ? 0.5 : 1)
                        .accessibilityIdentifier("agent-question-option-\(index + 1)")
                    } else {
                        optionRow(
                            option,
                            number: index + 1,
                            primary: primary,
                            checked: question.multiSelect ? false : nil,
                            selected: false,
                            expandable: expandsInline(option),
                            inlineOpen: false
                        )
                    }
                    if answerable, inlineOpen {
                        inlineField(for: option)
                    }
                }
            }
        }
    }

    /// EXP-820: the inline field a free-text row or the plan's reject row
    /// expands into — the composer's glass field in miniature with the round
    /// Send mark. A free-text answer needs text; the plan's feedback Send is
    /// always live (empty = a plain reject, exactly what the row's tap did).
    private func inlineField(for option: AgentQuestionOption) -> some View {
        let planFeedback = !option.freeText
        let text = inlineText.trimmingCharacters(in: .whitespacesAndNewlines)
        let canSend = planFeedback || !text.isEmpty
        return GlassTextField(
            planFeedback ? AgentFeed.planFeedbackPlaceholder : AgentFeed.freeTextPlaceholder,
            text: $inlineText,
            lines: 1...5,
            verticalPadding: 8,
            accessibilityIdentifier: "agent-question-inline-answer"
        ) {
            EmptyView()
        } trailing: {
            GlassComposerSubmitButton(
                AppIcons.uiSubmit,
                accessibilityLabel: "Send",
                enabled: canSend
            ) {
                sendInline(option)
            }
        }
        .font(.caption)
        .focused($inlineFocused)
        .submitLabel(.send)
        .onSubmit { sendInline(option) }
        .onAppear { inlineFocused = true }
    }

    /// EXP-820: no in-card lock captions once the ask is over (`askOpen`) —
    /// a completed stepper used to keep "Answer sent" spinning under its
    /// last step. What is left here is the multi-select Submit and the
    /// lock/retry captions.
    @ViewBuilder
    private var trailingActions: some View {
        if answerable, needsExplicitSubmit {
            // Multi-select submits every picked key at once — the solid
            // primary pill, never a blue one.
            let disabled = locked || picked.isEmpty
            GlassPill(
                submitTitle,
                mode: .action { submit() },
                primary: true,
                enabled: !disabled
            )
        }
        if locked, !question.resolved, askOpen {
            HStack(spacing: 6) {
                if pending {
                    ProgressView().controlSize(.small).tint(.white)
                } else {
                    // Confirmed injected — the card stays locked until the
                    // desktop retires it with `question_resolved`.
                    AppIcon(AppIcons.uiCheck, size: 11)
                        .foregroundStyle(DesignTokens.Semantic.green)
                }
                Text(pending ? "Sending…" : "Answer sent")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        } else if failed, !question.resolved, answerable {
            // The optimistic lock expired with no `answer_ack` — say WHY the
            // step re-surfaced instead of silently rolling back (EXP-334).
            Text("No confirmation from the desktop. Pick again to retry.")
                .font(.caption2)
                .foregroundStyle(DesignTokens.Semantic.yellow)
        }
    }

    private func pick(_ option: AgentQuestionOption) {
        guard !locked else { return }
        if expandsInline(option) {
            // EXP-820: the row opens (or folds) its inline field; nothing is
            // sent until its Send.
            if inlineKey == option.key {
                inlineKey = nil
                inlineFocused = false
            } else {
                inlineKey = option.key
            }
            return
        }
        if question.multiSelect {
            if let index = picked.firstIndex(of: option.key) {
                picked.remove(at: index)
            } else {
                picked.append(option.key)
            }
            // The keys batch into the submit frame.
            return
        }
        picked = [option.key]
        onAnswer([option.key], nil)
    }

    /// EXP-820: the inline field's Send. A free-text row answers with the
    /// text under its key (`sendAnswer` makes the text the label); the plan's
    /// reject row answers on its key and the text follows as the next message
    /// (`onAnswerThenSend`), or is a plain reject when empty.
    private func sendInline(_ option: AgentQuestionOption) {
        guard !locked else { return }
        let text = inlineText.trimmingCharacters(in: .whitespacesAndNewlines)
        if option.freeText {
            guard !text.isEmpty else { return }
            picked = [option.key]
            onAnswer([option.key], text)
        } else if text.isEmpty {
            picked = [option.key]
            onAnswer([option.key], nil)
        } else if let onAnswerThenSend {
            picked = [option.key]
            onAnswerThenSend([option.key], text)
        } else {
            picked = [option.key]
            onAnswer([option.key], nil)
        }
        inlineKey = nil
        inlineText = ""
        inlineFocused = false
    }

    private func submit() {
        guard !locked, !picked.isEmpty else { return }
        onAnswer(picked, nil)
    }

    /// One option row (EXP-788): the whole width is the hit target (a
    /// plain-style button only hit-tests what it draws, EXP-588), glassRow
    /// rather than the capsule button whose height-derived radius clipped a
    /// two-line description into an ellipse (EXP-274). EXP-820: the primary
    /// row is the solid `Palette.primary` fill with `primaryForeground` text
    /// (the app's primary button, no blue anywhere); a picked or expanded row
    /// is the active glass (`fillActive` + `strokeActive`).
    private func optionRow(
        _ option: AgentQuestionOption,
        number: Int,
        primary: Bool,
        checked: Bool?,
        selected: Bool,
        expandable: Bool,
        inlineOpen: Bool
    ) -> some View {
        let labelColor: Color = primary ? DesignTokens.Palette.primaryForeground : .white
        let quietColor: Color = primary
            ? DesignTokens.Palette.primaryForeground.opacity(TextOpacity.secondary)
            : .white.opacity(TextOpacity.tertiary)
        let row = HStack(alignment: .top, spacing: 8) {
            if let checked {
                // Multi-select rows carry an explicit checkbox (EXP-529) —
                // the glassRow tint alone was too subtle to read the picked
                // set off the card. Android parity (QuestionOptionLabel).
                AppIcon(checked ? AppIcons.uiSelected : AppIcons.uiUnselected, size: 13)
                    .foregroundStyle(
                        .white.opacity(checked ? TextOpacity.primary : TextOpacity.tertiary)
                    )
                    .padding(.top, 2)
            } else if number <= Self.numberedRows {
                numberChip(number, primary: primary)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(option.label)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(labelColor)
                    .multilineTextAlignment(.leading)
                if let description = option.description, !description.isEmpty {
                    Text(description)
                        .font(.caption2)
                        .foregroundStyle(quietColor)
                        .multilineTextAlignment(.leading)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if expandable {
                // EXP-820: this row types — the chevron says it unfolds.
                AppIcon(inlineOpen ? AppIcons.uiChevronUp : AppIcons.uiChevronDown, size: 11)
                    .foregroundStyle(quietColor)
                    .padding(.top, 2)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .contentShape(Rectangle())
        return Group {
            if primary {
                row.background(
                    DesignTokens.Palette.primary,
                    in: RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                )
            } else {
                row.glassRow(isActive: selected || inlineOpen)
            }
        }
    }

    /// The 1..9 chip — the keystroke the desktop would take. On the primary
    /// row it inverts onto the solid fill; a quiet glass square elsewhere.
    private func numberChip(_ number: Int, primary: Bool) -> some View {
        Text("\(number)")
            .font(.caption2.weight(.semibold).monospacedDigit())
            .foregroundStyle(
                primary ? DesignTokens.Palette.primaryForeground : .white.opacity(TextOpacity.secondary)
            )
            .frame(width: 18, height: 18)
            .background(
                primary
                    ? DesignTokens.Palette.primaryForeground.opacity(0.12)
                    : GlassTokens.fillActive,
                // EXP-850 §13: the number-key chip wears `radius.sm` ×4 (the
                // option row itself is `radius.md`, never a capsule).
                in: RoundedRectangle(cornerRadius: DesignTokens.Radius.sm)
            )
            .accessibilityHidden(true)
    }
}

/// Tool-call headline — compact single line, consecutive rows visually tight.
private struct ToolRow: View {
    let name: String
    let detail: String?
    /// EXP-785: the call failed — the row tints red (web parity) so a
    /// collapsed group's "1 failed" has a row to point at once expanded.
    var failed: Bool = false
    /// EXP-787: a row INSIDE an expanded group keeps the compact inner rhythm
    /// this used to give every tool row; an outermost one is spaced by the
    /// transcript's gap ladder instead.
    var nested: Bool = false
    /// EXP-846: the call SETTLED — a `tool_update` status landed. What swaps an
    /// Exponential row's progressive caption ("Creating issue") for its done
    /// one ("Created issue").
    var settled: Bool = false
    /// EXP-846: the result the publisher distilled out of an Exponential MCP
    /// call's own answer. Nil for every other tool.
    var preview: AgentToolPreview? = nil
    /// EXP-846: the run's team + where a tapped issue preview goes.
    var refs: AgentIssueRefContext? = nil
    /// EXP-895: what an `execute` call PRINTED, as its settle published it —
    /// redacted and tail-cut by the publisher. Nil for every other call.
    var output: String? = nil
    /// EXP-895: this call is the ONE still running (`AgentFeed.liveToolRowId`).
    /// The transcript runs inside the flow, so this row — and only this row —
    /// opens itself.
    var live: Bool = false

    /// EXP-806/895: nil = "whatever the flow says", so the RUNNING row is open
    /// and every other row is the compact headline; a tap pins it either way.
    /// The pin drops on the live→settled edge, which is what folds a row away
    /// by itself once the transcript has moved past it. (A row the reader opened
    /// AFTER it settled stays open: `live` no longer moves.)
    @State private var pinned: Bool? = nil

    /// The disclosure's state. One toggle for one row: since EXP-916 an edit's
    /// patch belongs to the edited-files card, so the only evidence a tool row
    /// discloses is an `execute`'s output.
    private var showsDetail: Bool { pinned ?? live }
    /// Whether there is anything to disclose at all.
    private var hasDetail: Bool { output != nil }

    var body: some View {
        // EXP-846: one of OUR OWN MCP tools gets its own row — the Exponential
        // mark, the contract's caption and the result preview — because
        // `mcp__exponential__exponential_issues_create` told a reader nothing.
        // A FAILED call keeps the generic red tool row: a failure has no result
        // to show, and the failure is the thing to see.
        if !failed, let display = ExpToolDisplay.resolve(toolName: name) {
            ExpToolRow(
                display: display,
                subject: detail,
                settled: settled,
                preview: preview,
                refs: refs
            )
            .padding(.vertical, nested ? 2 : 0)
        } else {
            genericRow
        }
    }

    /// Every other tool: the neutral glyph, the tool's name, its detail, and —
    /// behind a disclosure — the evidence the call produced: an `execute`'s
    /// output. An `edit` never reaches here (EXP-916: it is a card row).
    private var genericRow: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !hasDetail {
                headline
            } else {
                Button { pinned = !showsDetail } label: {
                    headline.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(showsDetail ? "Hide the output" : "Show the output")
            }
            if showsDetail, let output {
                ToolOutputBlock(output: output, live: live)
                    .padding(.top, 4)
            }
        }
        .padding(.vertical, nested ? 2 : 0)
        // EXP-895: the live→settled edge drops the reader's pin, so the row
        // folds behind the flow instead of staying open forever.
        .onChange(of: live) { pinned = nil }
    }

    private var headline: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.codingTool, size: 11)
                .foregroundStyle(
                    failed ? DesignTokens.Semantic.red : .white.opacity(TextOpacity.tertiary)
                )
            Text(name)
                .transcriptToolText(.medium)
                .foregroundStyle(failed ? DesignTokens.Semantic.red : .white)
            if let detail {
                Text(Self.middleTruncate(detail))
                    .font(.caption2.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Spacer(minLength: 0)
            }
            // The disclosure sits on the TRAILING edge on purpose: a leading
            // chevron would indent the evidence-carrying rows out of line with
            // every other tool row in the same run.
            if hasDetail {
                AppIcon(showsDetail ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
    }

    /// Middle-truncate a tool detail (paths etc.) so head AND tail stay
    /// readable (Android AgentSessionScreen parity).
    private static func middleTruncate(_ s: String, max: Int = 72) -> String {
        guard s.count > max else { return s }
        let head = max * 2 / 3
        let tail = max - head - 1
        return String(s.prefix(head)) + "…" + String(s.suffix(tail))
    }
}

/// EXP-846 — an **Exponential MCP call** in the transcript.
///
/// The agent works the tracker constantly (`exponential_issues_create`,
/// `exponential_pr_open`, `exponential_comments_create`, …) and those rows used
/// to read as `mcp__exponential__exponential_issues_create` beside a generic
/// tool glyph. This row is our own: the Exponential mark, the contract's
/// caption (progressive while the call is in flight, done once it settled), the
/// subject the publisher named, and — on completion — a small preview of what
/// the call actually produced, keyed on the contract's result KIND:
///
///   issue                         → the issue pill the `#EXP-1` refs draw,
///                                   resolved off the store and tappable
///   pr                            → a link row opening the pull request
///   list                          → "N results"
///   session/board/action/automation → a name chip
///   none                          → the caption alone
///
/// Nothing is forced: a call that reported no preview is the mark plus its
/// caption, which is already the whole story for a delete or an update.
/// Hand-mirrored ×4 (web `agent-feed` rows, desktop `steer` feed, Android
/// `AgentFeed`); every string comes from `ExpToolDisplay`, never from here.
private struct ExpToolRow: View {
    let display: ExpToolDisplay
    /// The subject the publisher put on the call's `detail` (the contract's
    /// `subjectKey` input — an issue title, a board name).
    let subject: String?
    let settled: Bool
    let preview: AgentToolPreview?
    let refs: AgentIssueRefContext?

    @Environment(\.openURL) private var openURL

    private var trimmedSubject: String? {
        guard let subject else { return nil }
        let text = subject.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                ExpLogoMark(size: 11)
                Text(display.caption(settled: settled))
                    .transcriptToolText(.medium)
                    .foregroundStyle(.white)
                if let trimmedSubject {
                    Text(trimmedSubject)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } else {
                    Spacer(minLength: 0)
                }
            }
            // Only a SETTLED call has a result; a preview that arrived early
            // (the publisher only sends one with a status) waits for it.
            if settled, let preview {
                previewRow(preview)
            }
        }
    }

    @ViewBuilder
    private func previewRow(_ preview: AgentToolPreview) -> some View {
        switch display.result {
        case .issue:
            ExpToolIssuePreview(
                identifier: preview.identifier, title: preview.title, refs: refs
            )
        case .pr:
            if let address = preview.url, let link = URL(string: address) {
                prRow(link, label: address)
            }
        case .list:
            if let count = preview.count {
                Text(count == 1 ? "1 result" : "\(count) results")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        case .session, .board, .action, .automation, .comment:
            // A name/identifier chip — never a bare uuid: an id the reader
            // cannot place says less than the caption already did.
            if let name = preview.title ?? preview.identifier {
                GlassPill(name, size: .sm)
            }
        case ExpToolResultKind.none:
            EmptyView()
        }
    }

    /// The PR the call opened/merged — tapping leaves for GitHub, the one
    /// surface that owns a pull request.
    private func prRow(_ url: URL, label: String) -> some View {
        Button {
            openURL(url)
        } label: {
            HStack(spacing: 6) {
                AppIcon(AppIcons.prOpen, size: 11)
                    .foregroundStyle(DesignTokens.Semantic.green)
                Text(label)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
                AppIcon(AppIcons.uiExternalLink, size: 10)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .glassRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Open the pull request")
    }
}

/// EXP-846: the issue an Exponential call landed on, drawn as the SAME pill a
/// `#EXP-1` ref wears — status glyph, mono identifier, title — and tappable
/// into the issue when the row has synced to this phone.
///
/// The title comes off the STORE when the issue is there (so it is the current
/// one, not whatever the agent saw), and falls back to the publisher's own
/// preview otherwise: a brand-new issue on a phone that has not synced it yet
/// still reads as `EXP-42 · Fix the merge queue` instead of vanishing.
private struct ExpToolIssuePreview: View {
    let identifier: String?
    let title: String?
    let refs: AgentIssueRefContext?

    @Environment(\.accountId) private var accountId

    /// The synced issue behind the identifier, resolved through the same memo
    /// the editors' chip pass uses. `@MainActor` because that memo is — a View's
    /// `body` already is, so only this helper needs saying so.
    @MainActor
    private var resolved: IssueRefLookup.Chip? {
        guard let refs, let identifier, !identifier.isEmpty else { return nil }
        return IssueRefChipCache.chip(
            identifier, scope: .team(id: refs.teamId), db: refs.db, accountId: accountId
        )
    }

    /// Nothing to open: an issue outside this phone's synced scope (or a call
    /// that named no identifier at all) stays a label, not a dead button.
    @MainActor
    private var openAction: (() -> Void)? {
        guard let chip = resolved, let refs else { return nil }
        return { refs.onOpen(chip.issueId) }
    }

    var body: some View {
        let chip = resolved
        // EXP-885: the SHARED badge, at the transcript's measure so it reads as
        // the same object as a `#EXP-1` chip in the narration above it. An
        // unresolved issue keeps the generic issue glyph in the muted token
        // colour — there is no status to show, but the chip is still a chip.
        let badge = IssueChip(
            identifier: identifier,
            title: chip?.title ?? title,
            iconName: chip?.status.iconName ?? AppIcons.uiIssue,
            statusColor: chip?.status.color,
            bodySize: DesignTokens.Transcript.bodySize,
            onTap: openAction
        )
        HStack(spacing: 0) {
            badge
            Spacer(minLength: 0)
        }
    }
}

/// EXP-895 — what one `execute` call printed, as its settle put it on the wire:
/// already redacted and tail-cut by the publisher, so this only has to be a
/// readable box. A cut log OPENS with the `\ N more lines truncated` marker (the
/// dropped lines were at the front, unlike a patch's trailing note), which reads
/// as its first line and needs no parsing.
///
/// Scrolled to the BOTTOM: the verdict is the last line, and it is why the output
/// is on the wire at all.
private struct ToolOutputBlock: View {
    let output: String
    /// EXP-910: the call is still RUNNING — show its TAIL
    /// (`AgentFeed.liveToolOutputTail`), not the whole log. A command that
    /// prints while it works owns the one open row, and an unbounded one owns
    /// the screen. The settled row (and the reader's own tap on it) still gets
    /// everything.
    var live: Bool = false

    private var shown: String {
        live
            ? AgentFeed.liveToolOutputTail(output, AgentFeed.liveToolOutputTailLines)
            : output
    }

    /// Web's `max-h-72` — tall enough to read a failure in, short enough that
    /// the prose after the call stays on screen.
    private static let maxHeight: CGFloat = 288
    private static let bottom = "exp-tool-output-bottom"

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(shown)
                        .font(.caption2.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Color.clear.frame(height: 0).id(Self.bottom)
                }
                .padding(8)
            }
            .frame(maxHeight: Self.maxHeight)
            // Web's `overscroll-contain`: a short log has nothing to scroll
            // here, so the drag belongs to the transcript.
            .scrollBounceBehavior(.basedOnSize)
            .background(Color.white.opacity(0.03))
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
            )
            .onAppear { proxy.scrollTo(Self.bottom, anchor: .bottom) }
        }
    }
}

/// A run of ≥2 consecutive tool calls collapsed into one "N tool calls" row
/// (EXP-97), expandable to the individual rows. While the run is the trailing
/// row of a live session, the latest call stays visible under the count so
/// the viewer still sees live progress.
private struct ToolGroupRow: View {
    let items: [AgentFeedItem]
    let liveTail: Bool
    /// EXP-846: what an Exponential call's issue preview resolves against (the
    /// run's team) and where a tap on it goes. Nil keeps the preview textual.
    var refs: AgentIssueRefContext? = nil

    @State private var expanded = false

    /// EXP-785: the collapsed caption is what the calls DID ("Ran 3 commands
    /// · edited 2 files"), the one summary every client derives from the
    /// contract fixture, not a bare count.
    private var caption: String {
        ToolGroupSummary.summarize(items.compactMap { item in
            guard case let .tool(_, _, detail, _, _, kind, _, failed, _, _, _) = item
            else { return nil }
            return ToolCallSummary(kind: kind ?? "other", detail: detail, failed: failed)
        })
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                expanded.toggle()
            } label: {
                HStack(spacing: 8) {
                    AppIcon(expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    AppIcon(AppIcons.codingTool, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Text(caption)
                        .transcriptToolText(.medium)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? "Collapse tool calls" : "Expand tool calls")
            if expanded {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(items) { item in
                        if case let .tool(
                            id, name, detail, _, _, _, settled, failed, _, preview, output
                        ) = item {
                            // EXP-895: inside the group too, only the RUNNING
                            // call is expanded — the same rule the top-level
                            // rows follow.
                            ToolRow(
                                name: name, detail: detail, failed: failed,
                                nested: true,
                                settled: settled, preview: preview, refs: refs,
                                output: output, live: liveTail && id == items.last?.id
                            )
                        }
                    }
                }
                .padding(.leading, 20)
            } else if liveTail, let last = items.last,
                      case let .tool(
                          _, name, detail, _, _, _, settled, failed, _, preview, output
                      ) = last {
                ToolRow(
                    name: name, detail: detail, failed: failed, nested: true,
                    settled: settled, preview: preview, refs: refs,
                    output: output, live: true
                )
                    .padding(.leading, 20)
            }
        }
    }
}

/// A subagent's run (EXP-249): its `started` marker plus every tool call the
/// desktop tagged with it, collapsed into one expandable row so a long subagent
/// detour never buries the main thread's activity. The header always shows the
/// agent type, a running/done status, and the delegation detail; expansion only
/// reveals the tool calls — so a run with none renders as a static row with no
/// chevron (EXP-350: the chevron used to expand to nothing).
private struct SubagentGroupRow: View {
    let run: AgentSubagentRun
    /// The trailing row of a live session — keep the latest call visible.
    let liveTail: Bool
    /// EXP-773: the group carries the subagent's own prose now, so it needs
    /// what an `AgentMarkdownText` needs.
    let context: AgentMarkdownContext

    @State private var expanded = false

    /// EXP-847: the spawn's description leads (`run.label`), the agent type
    /// is its fallback; the type stays as the header's secondary caption so a
    /// titled run still says WHICH agent is doing the work.
    private var title: String { run.label }

    /// EXP-847: what the subagent DID — the contract's `toolGroupSummary`
    /// ("Ran 3 commands · edited 2 files"), the same sentence a collapsed tool
    /// run wears and locked ×4 by the shared fixture. It replaced the bare "N
    /// tool calls", which said how much happened and never what.
    ///
    /// A replay evicts a subagent's tool rows FIRST (EXP-748), so a run whose
    /// rows are gone but whose publisher reported a count keeps the count
    /// sentence — summarising zero visible rows as "No tool calls" would
    /// contradict the row's own history. Empty = nothing to say yet.
    private var summary: String {
        let calls = run.items.compactMap { item -> ToolCallSummary? in
            guard case let .tool(_, _, detail, _, _, kind, _, failed, _, _, _) = item
            else { return nil }
            return ToolCallSummary(kind: kind ?? "other", detail: detail, failed: failed)
        }
        guard !calls.isEmpty else {
            let count = run.toolCount
            guard count > 0 else { return "" }
            return count == 1 ? "1 tool call" : "\(count) tool calls"
        }
        return ToolGroupSummary.summarize(calls)
    }

    /// The muted caption beside the title — the agent type, but only when it
    /// is not already the title.
    private var typeCaption: String? {
        run.title == nil ? nil : run.agentType
    }

    private var header: some View {
        HStack(spacing: 8) {
            if run.expandable {
                AppIcon(expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            AppIcon(AppIcons.codingSubagent, size: 11)
                .foregroundStyle(DesignTokens.Semantic.blue)
            Text(title)
                .transcriptToolText(.medium)
                .foregroundStyle(.white)
                .lineLimit(1)
            // EXP-847: the agent type as a secondary caption — only where the
            // title took its place in the lead.
            if let typeCaption {
                Text(typeCaption)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                    .lineLimit(1)
            }
            // EXP-847: the group's own work summary, where the bare count used
            // to be part of the title.
            if !summary.isEmpty {
                Text(summary)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            Text(run.done ? "done" : "running…")
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if run.expandable {
                Button {
                    expanded.toggle()
                } label: {
                    header.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(expanded ? "Collapse subagent" : "Expand subagent")
            } else {
                header
            }
            if let detail = run.detail {
                Text(detail)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(2)
                    .padding(.leading, 20)
            }
            // EXP-856: a second copy of this agent is running — the wire's own
            // sentence, kept whether the group is expanded or collapsed.
            if let duplicate = run.duplicateDetail {
                AgentDuplicateWarningRow(detail: duplicate)
                    .padding(.leading, 20)
            }
            if expanded {
                // EXP-773: the run's whole conversation in order — its prose
                // and the turns addressed to it, not just its tool calls.
                // EXP-916: its edits fold into the one edited-files card.
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(AgentFeed.laneRows(run.items)) { row in
                        SubagentLaneRow(row: row, context: context)
                    }
                }
                .padding(.leading, 20)
            } else if liveTail, let last = AgentFeed.laneRows(run.items).last {
                SubagentLaneRow(row: last, context: context)
                    .padding(.leading, 20)
            }
        }
    }
}

/// EXP-916 — one PROJECTED row of a subagent's lane: its edited-files card, or
/// one item of its conversation. The lane runs through `AgentFeed.laneRows`, so
/// a subagent's edits fold into the same card the main transcript draws instead
/// of one tool row per file.
struct SubagentLaneRow: View {
    let row: AgentFeedRow
    let context: AgentMarkdownContext
    /// EXP-787: inside an expanded group the conversation keeps its own
    /// compact rhythm; the focused-subagent list spaces its rows with the
    /// transcript's gap ladder instead.
    var nested: Bool = true

    var body: some View {
        switch row {
        case let .edits(items):
            // A digest inside someone else's transcript: no row of it is the
            // live one — the subagent's own tab is where its work is read.
            EditedFilesCard(items: items)
                .padding(.vertical, nested ? 2 : 0)
        case let .single(item):
            SubagentItemRow(item: item, context: context, nested: nested)
        default:
            EmptyView()
        }
    }
}

/// EXP-773: one row of a subagent's conversation — its prose, a turn
/// addressed to it, or one of its tool calls. Anything else a group somehow
/// collected renders nothing rather than crashing the feed.
struct SubagentItemRow: View {
    let item: AgentFeedItem
    let context: AgentMarkdownContext
    /// EXP-787: inside an expanded group the conversation keeps its own
    /// compact rhythm; the focused-subagent list spaces its rows with the
    /// transcript's gap ladder instead.
    var nested: Bool = true

    var body: some View {
        content.padding(.vertical, nested ? 2 : 0)
    }

    @ViewBuilder
    private var content: some View {
        switch item {
        case let .tool(_, name, detail, _, _, _, settled, failed, _, preview, output):
            // A subagent's rows are a digest inside the main transcript — its
            // own tab is where its work is read — so none of them is the live
            // row here; the reader's tap still opens one.
            ToolRow(
                name: name, detail: detail, failed: failed,
                settled: settled, preview: preview, refs: context.issueRefs,
                output: output
            )
        case let .narration(_, text, _, _):
            NarrationBubble(text: text, context: context)
        case let .userMessage(_, text, _):
            UserMessageBubble(text: text, context: context)
        default:
            EmptyView()
        }
    }
}

/// A stray subagent marker (EXP-249) — a `completed` whose `started` fell off
/// the top of the feed, or a start with nothing published under it yet.
private struct SubagentRow: View {
    let agentType: String
    /// EXP-847: the spawn's own description, when it named one.
    var title: String? = nil
    let status: AgentSubagentStatus
    let detail: String?

    /// EXP-847: the title leads, the agent type is the fallback — mirroring
    /// `AgentSubagentRun.label`.
    private var label: String { title ?? agentType }

    var body: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.codingSubagent, size: 11)
                .foregroundStyle(DesignTokens.Semantic.blue)
            Text(status == .completed ? "\(label) finished" : "\(label) started")
                .transcriptToolText(.medium)
                .foregroundStyle(.white)
            if let detail {
                Text(detail)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Spacer(minLength: 0)
            }
        }
    }
}

/// A permission prompt the agent hit (EXP-249) — INFORMATIONAL only: the
/// approval lives in the desktop's own TUI, there is nothing to answer here.
private struct PermissionRow: View {
    let tool: String
    let detail: String?

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.uiPermission, size: 11)
                .foregroundStyle(DesignTokens.Semantic.yellow)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 2) {
                Text("Permission requested · \(tool)")
                    .transcriptToolText(.medium)
                    .foregroundStyle(.white)
                if let detail {
                    Text(detail)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(2)
                }
                Text("Approve on the desktop, or reply below to continue.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Spacer(minLength: 0)
        }
    }
}

/// EXP-724: the quiet marker a finished compaction leaves in the feed, so the
/// gap in the conversation above it stays explained after the strip is gone.
/// Centered and muted — it is punctuation, not a turn. `compactedLabel` is
/// byte-identical on all four clients.
private struct CompactionMarkerRow: View {
    var body: some View {
        HStack(spacing: 6) {
            Spacer(minLength: 0)
            AppIcon(AppIcons.codingCompact, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(AgentFeed.compactedLabel)
                .transcriptToolText()
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
    }
}

/// EXP-724: a steered slash command in the feed. It is a control action, not
/// prose, so it gets a compact pill on the human side of the conversation
/// instead of a markdown bubble: `square-slash` · mono `/name` · muted args.
private struct CommandPill: View {
    let command: SlashCommand
    /// The message as sent — everything after the command token is its
    /// argument.
    let text: String

    /// Everything after the command token — the match ran on that first
    /// whitespace token, so the rest of the message is exactly the argument.
    private var arguments: String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let space = trimmed.firstIndex(where: { $0.isWhitespace }) else { return "" }
        return String(trimmed[space...]).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        HStack {
            Spacer(minLength: 32)
            HStack(spacing: 6) {
                AppIcon(AppIcons.codingCommand, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(command.token)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white)
                if !arguments.isEmpty {
                    Text(arguments)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .glassRow()
        }
    }
}

// MARK: - Follow-scroll geometry

/// Derives "pinned to the bottom" from the feed's scroll geometry. iOS 18+
/// reads onScrollGeometryChange — the supported scroll-observation API; the
/// EXP-70 content-frame preference (still emitted by the feed's background)
/// stopped re-evaluating during scrolls on current iOS, leaving `atBottom`
/// stuck true so the "Jump to bottom" pill never appeared and follow-scroll
/// couldn't be escaped (EXP-212). Pre-18 keeps the preference path.
private struct FollowPinTracker: ViewModifier {
    @Binding var atBottom: Bool
    /// Within this many points of the bottom still counts as pinned.
    let slack: CGFloat
    /// Re-assert the bottom pin (a scrollTo) after the CONTENT grew while
    /// pinned — in-place feed mutations (a question card filling in, a tool
    /// group's live tail) change no row count, so the feed-count follow
    /// misses them. Without this the growth pushed the bottom out from
    /// under a pinned viewer, `atBottom` flipped false with no user gesture,
    /// and the "Jump to bottom" pill sat on screen permanently (EXP-272).
    let repin: () -> Void

    /// A user scroll gesture is driving the offset (drag or its momentum).
    @State private var userScrolling = false
    /// The latest geometry sample's pin verdict — lets the phase observer
    /// re-pin once a user scroll ends with `atBottom` still true but the
    /// viewport left short of the end (content grew under the finger).
    @State private var lastPinned = true
    /// A growth repin is already queued for the next run-loop turn.
    @State private var repinQueued = false
    /// Growth repins issued inside the CURRENT burst window. Bounded: a repin
    /// that keeps landing short (lazy rows realising under it, an OS layout
    /// bug) must not become a relayout storm — build 94 was watchdog-killed
    /// with the main thread 100% in lazy-stack layout and +380MB of view-list
    /// copies in 84s.
    @State private var growthRepins = 0
    /// When the current burst window opened. The budget is a RATE (n per
    /// window), never a count reset by the geometry: a repin is what puts the
    /// feed's end back in view, so "the end is in view" is the storm's own
    /// output — re-arming the budget there let the loop re-arm itself and spin
    /// the main thread for good (EXP-943: a live run streaming into the feed
    /// wedged the layout, so rows drew at stale positions over one another and
    /// the transcript could not be scrolled to its end).
    @State private var burstOpenedAt = Date.distantPast
    private static let maxGrowthRepins = 8
    /// Long enough that one layout storm cannot outlast it, short enough that
    /// a streaming run keeps following (8 repins per window is far more than
    /// the eye resolves).
    private static let growthBurstWindow: TimeInterval = 2

    /// Queue ONE repin for the next run-loop turn. The growth observer runs
    /// inside the scroll view's layout transaction; calling scrollTo there
    /// re-enters layout synchronously (seen in the build 92 watchdog
    /// stack: onScrollGeometryChange action → scrollTo → LazyStack layout),
    /// and every scrollTo that realises more lazy rows changes the content
    /// height, which fires the observer again before the run loop ever
    /// turns. Deferring breaks the re-entrancy and coalesces a burst of
    /// growth samples into a single scroll.
    private func queueGrowthRepin() {
        guard !repinQueued else { return }
        let now = Date()
        if now.timeIntervalSince(burstOpenedAt) >= Self.growthBurstWindow {
            burstOpenedAt = now
            growthRepins = 0
        }
        guard growthRepins < Self.maxGrowthRepins else { return }
        repinQueued = true
        growthRepins += 1
        Task { @MainActor in
            repinQueued = false
            guard atBottom, !userScrolling else { return }
            repin()
        }
    }

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content
                .onScrollPhaseChange { _, newPhase in
                    userScrolling = newPhase == .tracking
                        || newPhase == .interacting
                        || newPhase == .decelerating
                    // A gesture is the one input the storm cannot fake, so it
                    // opens a fresh window (EXP-943).
                    if userScrolling {
                        growthRepins = 0
                        burstOpenedAt = Date()
                    }
                    // The growth observer skips repins during user scrolls
                    // (EXP-306); catch up once the gesture settles so a
                    // followed feed never idles a few points shy of its end.
                    if newPhase == .idle, atBottom, !lastPinned {
                        repin()
                    }
                }
                .onScrollGeometryChange(for: FeedPinMetrics.self) { geometry in
                    // Pinned ⇔ the content's bottom edge is within slack of
                    // the visible rect's bottom edge (Android parity: last
                    // item bottom vs viewport end). `visibleRect` already
                    // accounts for the content insets, so nothing is
                    // reconstructed from contentOffset/containerSize/insets —
                    // the EXP-212 max-offset formula overstated the reachable
                    // maximum whenever an inset (nav bar, keyboard, safe area)
                    // didn't line up, and the true bottom then read as "not
                    // pinned", stranding the "Jump to bottom" pill (EXP-591).
                    // A feed shorter than the viewport and a bounce past the
                    // end both come out negative, i.e. pinned (EXP-242).
                    let below = geometry.contentSize.height - geometry.visibleRect.maxY
                    return FeedPinMetrics(
                        pinned: below <= slack,
                        offset: geometry.contentOffset.y,
                        height: geometry.contentSize.height
                    )
                } action: { old, new in
                    lastPinned = new.pinned
                    // The rule itself is pure (ExpCore's FeedFollowPolicy):
                    // reaching the bottom re-arms follow, LEAVING it is the
                    // user's alone (Android parity: only drags flip `follow`),
                    // and a pin the CONTENT produced under a stationary reader
                    // holds — EXP-656, where a staged replay committing a
                    // shorter history pulled the bottom edge into slack range
                    // and scrolled a reader out of the plan they were reading.
                    switch FeedFollowPolicy.decide(
                        pinned: new.pinned,
                        atBottom: atBottom,
                        userScrolling: userScrolling,
                        offsetDelta: Double(new.offset - old.offset),
                        heightDelta: Double(new.height - old.height)
                    ) {
                    case .rearm:
                        if !atBottom { atBottom = true }
                    case .unpin:
                        atBottom = false
                    case .hold:
                        break
                    }
                }
                .onScrollGeometryChange(for: FeedGrowthMetrics.self) { geometry in
                    FeedGrowthMetrics(
                        height: geometry.contentSize.height,
                        below: geometry.contentSize.height - geometry.visibleRect.maxY
                    )
                } action: { old, new in
                    // NOTHING re-arms the budget here: `below <= 0` is what a
                    // repin PRODUCES, so re-arming on it handed the storm its
                    // own escape hatch (EXP-943). The rate window in
                    // `queueGrowthRepin` re-arms on time alone, and a user
                    // gesture re-arms in the phase observer.
                    // Only chase growth that actually pushed the bottom out
                    // of view; a scrollTo whose target is already visible is
                    // pure layout churn.
                    if new.height > old.height, new.below > 0, atBottom, !userScrolling {
                        queueGrowthRepin()
                    }
                }
        } else {
            content.onPreferenceChange(FeedBottomOverflowKey.self) { [atBottom = $atBottom] overflow in
                // Points of content extending below the viewport; ≤ slack
                // counts as pinned. Only the flip writes state.
                let pinned = overflow <= slack
                if atBottom.wrappedValue != pinned {
                    atBottom.wrappedValue = pinned
                }
            }
        }
    }
}

/// Scroll-geometry sample the iOS 18+ pin tracker acts on: pinned-to-bottom
/// plus the raw offset and content height, so the action can tell a user
/// scroll AWAY from the bottom (offset moved up) apart from content growth
/// un-pinning the geometry under a stationary finger (EXP-306) and from
/// content SHRINKING into a pin nobody scrolled into (EXP-656).
private struct FeedPinMetrics: Equatable {
    var pinned: Bool
    var offset: CGFloat
    var height: CGFloat
}

/// Content height plus how far the content's bottom edge sits below the
/// visible rect (≤ 0 when the end is on screen) — the growth observer's
/// sample.
private struct FeedGrowthMetrics: Equatable {
    var height: CGFloat
    var below: CGFloat
}

/// Points of feed content extending below the visible viewport — 0 when the
/// feed is pinned to the bottom, negative while bouncing past it. Pre-iOS-18
/// fallback input to FollowPinTracker.
private struct FeedBottomOverflowKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}
