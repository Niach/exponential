import ExpCore
import ExpUI
import GRDB
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The feed's scroll constants — outside the view (a generic type cannot hold
/// stored statics).
enum AgentSessionLayout {
    static let bottomAnchor = "feed-bottom"
    static let feedCoordSpace = "feed-scroll"
    /// EXP-787: the transcript's own inset, INSIDE the reading column.
    static let gutter: CGFloat = 16
    /// Within this many points of the bottom still counts as pinned (Android
    /// carries 96dp, EXP-529): a pixel-tight slack made the "Jump to bottom"
    /// pill hard to dismiss — a short drag had to land on the exact bottom to
    /// re-pin, so it lingered while the list visually WAS at the end (EXP-588).
    /// iOS gets a little more room than Android (EXP-591): the pill should
    /// only appear after a deliberate scroll-up and hide again well before
    /// the finger reaches the true end.
    static let followSlack: CGFloat = 120
}

// EXP-927 (wire doc §2c "Alignment"): the transcript is a reading measure, and
// everything stacked with it above the composer — the conversation tab row and
// every strip block — sits in the SAME column. One helper, the shared
// `DesignTokens.Transcript.maxWidth` cap: a second constant is how a phone's
// gutters and an iPad's column drift apart.
extension View {
    /// Gutter + cap, centred in whatever is wider.
    func transcriptColumn() -> some View {
        padding(.horizontal, AgentSessionLayout.gutter).transcriptColumnWidth()
    }

    /// The cap alone — for a row that must carry its gutter INSIDE itself (a
    /// horizontally scrolling tab strip: the padding belongs to the content,
    /// not to the scroll view).
    func transcriptColumnWidth() -> some View {
        frame(maxWidth: DesignTokens.Transcript.maxWidth, alignment: .leading)
            .frame(maxWidth: .infinity)
    }
}

/// EXP-1295: the `agentSession` route's screen — waits for the synced
/// `sessions` row, then hands it to `AgentSessionView` (whose model
/// observes it from there on); none yet = `run-not-found`.
struct AgentSessionScreen: View {
    let sessionId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var row: SessionEntity?
    @State private var missing = false

    var body: some View {
        Group {
            if let row {
                AgentSessionView(accountId: accountId, session: row)
            } else {
                ZStack {
                    AppBackground()
                    if missing {
                        EmptyState(.runNotFound)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                            .accessibilityIdentifier("session-not-found")
                    } else {
                        // EXP-1275 (D8): a first load is skeleton rows.
                        SkeletonRows(rows: 3, rowHeight: 28, dividers: false)
                            .transcriptColumn()
                            .frame(maxHeight: .infinity, alignment: .top)
                            .padding(.top, 8)
                    }
                }
            }
        }
        // OBSERVED, not read once: a push tap can land before the row syncs,
        // and the screen resolves the moment it arrives. Only the first row
        // is kept — the session view observes it from there on.
        .task(id: sessionId) {
            guard row == nil, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
            let id = sessionId
            let observation = ValueObservation.tracking { db in
                try SessionQueries.session(db: db, id: id)
            }
            do {
                for try await found in observation.values(in: pool) {
                    if let found {
                        row = found
                        return
                    }
                    missing = true
                }
            } catch {
                missing = true
            }
        }
    }
}

/// EXP-1295: a SESSION's raw work — today's session view, opened from its
/// card in a thread (or any session ref). A chat-style view of the session
/// over the relay's scrubbed activity channel: narration bubbles, compact tool
/// rows, collapsible subagent runs, question cards. NO terminal rendering.
///
/// The header is the back button, a back row naming the THREAD (its root,
/// EXP-1310) over the session's name, and Stop (`sessions.stop`); the composer box sends a `messages` row
/// (`sessions.send({sessionId})`). The relay stays the LIVE layer (streaming
/// text, tool lines, usage). Read-only for a viewer who may not post (a
/// teammate's session on a private device).
struct AgentSessionView: View {
    let accountId: String
    /// The row the screen opened on — the model observes it from there on.
    let session: SessionEntity
    /// EXP-1343: a page of the thread screen — no header of its own (the
    /// thread's band + strip are the ONE header; its Stop rides the thread
    /// header's trailing slot through `onModel`).
    var embedded = false
    /// EXP-1343: the claimed model, handed to the host (nil once the page
    /// left), keyed by this session's id.
    var onModel: ((String, AgentSessionModel?) -> Void)? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL
    @Environment(\.toaster) private var toaster
    @Environment(\.pushRoute) private var pushRoute
    /// EXP-1347: inside a thread, an issue that is one of its tabs opens
    /// that tab (the innermost outlet).
    @Environment(\.threadIssueOutlet) private var threadIssueOutlet
    /// A cache of the SteerSessionStore lookup (EXP-621) — the model itself is
    /// app-scoped, so this view neither creates nor tears it down.
    @State private var model: AgentSessionModel?
    /// This view's ONE claim on that model (`attach` / `detach` are counted).
    @State private var claimed = false
    /// EXP-688: the Usage sheet — the per-window cards, opened by the usage
    /// RING.
    @State private var showUsageSheet = false
    /// EXP-849: the usage wall's account picker.
    @State private var showAccountSwitch = false
    /// Whether the feed is scrolled to (within slack of) its bottom —
    /// auto-scroll only while pinned; scrolling up pauses follow and surfaces
    /// the "Jump to bottom" pill.
    @State private var atBottom = true
    /// EXP-356: the selected conversation tab — nil is the main agent; a
    /// subagent id focuses that agent's stream. Falls back to Main whenever
    /// the id vanishes from the feed (an `activity_reset` replay).
    @State private var agentTab: String?
    /// Wave D: the `+`'s Photo | File menu, the photo picker and the Files
    /// importer it opens (`SessionComposerCard` presents them).
    @State private var attach = SessionComposerAttach()
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
    /// EXP-1316: the floating bottom band's measured height (the feed's
    /// bottom inset) — a pinned feed re-pins as it changes.
    @State private var bottomBandHeight: CGFloat = 0
    /// Set by `expandComposer`, cleared once the editor takes focus (or the
    /// retry window passes): the EXP-931 idle fold stands down meanwhile.
    @State private var composerFocusPending = false
    /// EXP-820: per ask id, the wire id of the EARLIER step being re-answered
    /// ("go back" in the stepper), or absent while the current step shows.
    /// View state, not model state: it is a place in the card, and a fresh
    /// screen starts on the current step.
    @State private var editingSteps: [String: String] = [:]
    /// EXP-1310: the Stop confirm (`Prompts.StopRun`, ×4 with Android).
    @State private var stopConfirm = false
    /// EXP-1280: this session's OWN tool-posted cards (`exponential_plan_propose`
    /// / `exponential_sessions_ask`: a `plan` / `question` message with no
    /// mirrored ACP ask) — the relay has no open ask for them, so they answer
    /// through `sessions.answer`, in the ask dock like a thread's.
    @State private var toolAsks: [ThreadRow] = []
    @State private var toolAnsweringId: String?
    @Environment(\.motion) private var motion

    /// EXP-746: Usage opens on EITHER half — the machine's rate-limit report
    /// (EXP-484) or this session's own context/spend off the relay.
    private var hasUsage: Bool {
        model?.runUsage != nil || model?.sessionUsage != nil || model?.runAccount != nil
    }

    // Three small chains instead of one long one. The whole modifier chain is
    // ONE expression to the type checker, and at this view's size that budget
    // has been blown twice already (#644, #656) — each time by a condition
    // spelled out inside one of its closures, and each fix bought exactly one
    // wave of headroom. Every group below is its own inference context, so a
    // new modifier costs its group and not the whole view. Nesting reads
    // inside out; the order of application is unchanged.
    var body: some View {
        withChrome(withSheets(withLifecycle(withAlerts(sessionContent))))
            // EXP-1172: a settled `sessions_show` row finds its picture in
            // the thread's Guide (the ROOT's `guide`).
            .environment(\.sessionResultsRaw, model?.root?.guide)
    }

    private var sessionContent: some View {
        ZStack {
            AppBackground()
            VStack(spacing: 0) {
                if let model {
                    runFace(model)
                } else {
                    Spacer()
                }
            }
        }
    }

    /// The transcript and the composer band, which FLOATS over the feed
    /// (EXP-1161): the transcript scrolls beneath it, and the feed's scroll
    /// view takes the band's height as its bottom inset so the follow pin and
    /// "Jump to bottom" land on the true end.
    @ViewBuilder
    private func runFace(_ model: AgentSessionModel) -> some View {
        feedBody(model)
            .safeAreaInset(edge: .bottom, spacing: 0) {
                // EXP-1358: the band is its OWN observation scope. Built
                // inline, every draft read it makes (`trimmedDraft`,
                // `slashMatches`, `hasContent`) was tracked by THIS view's
                // body, so each keystroke re-rendered the whole transcript.
                BodySlice {
                    bottomBand(model)
                }
                // EXP-1316: the band's measured height IS the feed's
                // bottom inset; a pinned feed re-pins when it changes
                // (the composer expanding, the keyboard, late layout).
                .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { height in
                    bottomBandHeight = height
                }
            }
    }

    /// Everything that sits on the feed's bottom edge: the standing banners,
    /// the strips and the composer bar. Each row floats over the scrolling
    /// transcript, so each carries an opaque surface of its own.
    private func bottomBand(_ model: AgentSessionModel) -> some View {
        VStack(spacing: 0) {
            banners(model)
            rateLimitBanner(model)
            compactionStrip(model)
            // EXP-861: the messages the device holds until the turn ends, each
            // with an X that revokes it. Absent when nothing is queued or the
            // session is over.
            if !model.queued.isEmpty, !model.isOver {
                AgentQueueStrip(messages: model.queued) { id in
                    model.unqueue(id)
                }
            }
            // EXP-1321: the background lines, waits and task list moved
            // into THE Queue at the transcript's end (`TaskQueueView`).
            askDock(model)
            // EXP-1211: the agent's quick replies, directly above the
            // composer; gone whenever the composer is.
            if !bandRetired(model) {
                SessionComposerReplies(sessionId: session.id)
                    .environment(\.accountId, accountId)
            }
            bottomBar(model)
        }
        // EXP-1162: the bottom edge strip behind the band — not behind an
        // open composer card, which is opaque and rides the keyboard.
        // EXP-1256 (D15): in the page's own ground — no tint, no blur.
        .floatingBarEdge(bandRetired(model) || !composerOpen(model), ground: true)
    }

    // MARK: - Header

    /// The nav bar: the system back button, a two-line title (EXP-1310, web
    /// `session-header`, Android `SessionHeaderBar`): the back row naming
    /// the THREAD over the session's name; and Stop while the session is
    /// live and this viewer may stop it (EXP-942: a NATIVE bar button, not a
    /// glass pill — the bar wraps its items in its own capsule).
    @ViewBuilder
    private func withChrome(_ content: some View) -> some View {
        if embedded {
            content
        } else {
            standaloneChrome(content)
        }
    }

    private func standaloneChrome(_ content: some View) -> some View {
        content
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.hidden, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .principal) { headerTitle }
                // Always mounted (EXP-942: an action-edge item that comes and
                // goes sometimes failed to reappear); draws nothing while
                // there is nothing to stop.
                ToolbarItem(placement: .topBarTrailing) { stopButton }
            }
            // EXP-240: the confirm on a zero-size node of its own.
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(isPresented: $stopConfirm) {
                        GlassAlert(prompt: Prompts.StopRun.copy(), handlers: [
                            "stop": { model?.stop() },
                        ])
                    }
            }
    }

    private var headerTitle: some View {
        VStack(spacing: 1) {
            // The way back to the thread (a thread already on the stack is
            // popped back to — `pushRoute`'s rule).
            if let threadTitle = model?.threadTitle, !threadTitle.isEmpty {
                Button {
                    pushRoute(.thread(accountId: accountId, threadId: session.rootId))
                } label: {
                    HStack(spacing: 2) {
                        AppIcon(AppIcons.uiChevronLeft, size: 11)
                        Text(threadTitle)
                            .font(.caption)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("session-back")
            }
            Text(model?.title ?? session.title ?? SessionTitle.Copy.untitled)
                .font(.headline)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
                .accessibilityIdentifier("session-title")
        }
    }

    /// The ending verb is the word "Stop" on every client — it stops the
    /// session (`sessions.stop`), confirmed first (`Prompts.StopRun`).
    @ViewBuilder
    private var stopButton: some View {
        if let model, model.canStop {
            // Text, not a Label: a bar item collapses a Label to its icon.
            Button(role: .destructive) { stopConfirm = true } label: {
                Text("Stop").fontWeight(.medium)
            }
            .tint(DesignTokens.Semantic.red)
            .accessibilityLabel("Stop the session")
            .accessibilityIdentifier("session-stop")
        }
    }

    /// The confirm a `/`-command asks for.
    private func withAlerts(_ content: some View) -> some View {
        content
            // EXP-724: `/clear` discards the conversation, so confirm rows
            // confirm before the message goes out. Copy is byte-identical ×4.
            // EXP-240: on a zero-size node of its own.
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(item: $slashConfirm) { command in
                        GlassAlert(
                            title: SlashCommands.confirmTitle(command),
                            message: SlashCommands.confirmBody,
                            actions: [
                                GlassAlertAction("Cancel", role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                                GlassAlertAction(SlashCommands.confirmButton(command), role: .destructive, id: "confirm") {
                                    if let model { Task { await model.sendDraft() } }
                                },
                            ]
                        )
                    }
            }
    }

    /// Attach/detach, the photo picker, and the state changes the screen
    /// reacts to. Their bodies live in methods below for the same reason the
    /// chain is split at all.
    private func withLifecycle(_ content: some View) -> some View {
        content
            .task(id: session.id) { await observeToolAsks() }
            // EXP-1031: the transient outcomes land as toasts.
            .onChange(of: model?.stopError) { _, message in
                guard let message else { return }
                toaster.error(FeedbackStates.Errors.title(verb: "stop", thing: "the session"), description: message)
            }
            .onChange(of: model?.sendError) { _, message in
                guard let message else { return }
                toaster.error(FeedbackStates.Errors.title(verb: "send", thing: "the message"), description: message)
            }
            .onChange(of: model?.switchError) { _, message in
                guard let message else { return }
                toaster.error(FeedbackStates.Errors.title(verb: "switch", thing: "the account"), description: message)
            }
            .onChange(of: model?.moveError) { _, message in
                guard let message else { return }
                toaster.error(SessionDeviceMove.failureToast, description: message)
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
            // EXP-931: and the same fold the moment the agent starts a turn
            // under an empty, unfocused composer.
            .onChange(of: model?.agentWorking) { _, _ in
                if let model { collapseIdleComposer(model) }
            }
            .onAppear {
                // The socket owner is app-scoped (EXP-621): popping back to this
                // screen re-attaches to the SAME model, so the feed is already
                // there, the composer still holds its draft, and there is no
                // connect phase to sit through. ONE claim per view.
                guard !claimed else { return }
                claimed = true
                model = deps.attachSteerModel(accountId: accountId, session: session)
                onModel?(session.id, model)
                // EXP-1323: opening a session view reads it (the Threads
                // list's bold child row).
                markRead()
            }
            // A newer agent message read here is read: re-stamp the session
            // (the Android/Thread rule — keyed on the newest agent message).
            .onChange(of: model?.newestAgentMessageKey) { _, newest in
                if newest != nil { markRead() }
            }
            .onDisappear {
                // EXP-802: the DRAFT outlives this screen, its focus must not —
                // the editor model would otherwise hand first responder straight
                // back on return and pop the keyboard over a screen nobody typed
                // into. (The text is untouched; only the caret's claim goes.)
                model?.draftEditor.setFocused(nil)
                guard claimed else { return }
                claimed = false
                onModel?(session.id, nil)
                // NOT a teardown: the store keeps the socket up while the session
                // runs and retires it once it is over (or falls off the cap).
                deps.steerSessions.detach(accountId: accountId, sessionId: session.id)
            }
    }

    /// The Usage sheet (EXP-688): every window the machine reported, grouped.
    private func withSheets(_ content: some View) -> some View {
        content
            .sheet(isPresented: $showUsageSheet) {
                if hasUsage, let model {
                    AgentUsageSheet(
                        agent: model.session?.cli,
                        // EXP-909: the login the session SPENDS, and ITS
                        // windows — not the machine's active login's.
                        runAccount: model.runAccount,
                        account: model.agentAccount,
                        usage: model.runUsage,
                        sessionUsage: model.sessionUsage,
                        // EXP-1051: and where that window went.
                        contextLayout: model.sessionContextLayout,
                        // EXP-909: one refresh of the session's login per open.
                        onOpen: { model.requestUsageRefresh() }
                    )
                }
            }
    }

    // MARK: - Lifecycle handlers

    // EXP-790: blur collapses the composer ONLY when nothing would be lost —
    // empty draft, no pending images, no picker mid-flight (presenting one
    // resigns first responder). Copied from IssueDetailBottomBar.
    private func draftEditingChanged(_ editing: Bool?) {
        // The focus `expandComposer` asked for has landed: the idle fold may
        // consider the composer again.
        if editing == true { composerFocusPending = false }
        guard composerExpanded, editing == false, let model else { return }
        guard !attach.busy, !model.hasContent else { return }
        withAnimation(motion.standard) { composerExpanded = false }
    }

    /// EXP-931: once the agent picks the turn up, an expanded composer with
    /// nothing in it and nobody typing in it is just a lid over the work bar —
    /// it stands down so the bar comes back. A focused field, a draft, a
    /// pending image or an open menu all keep it open; focusing or typing
    /// expands it again.
    private func collapseIdleComposer(_ model: AgentSessionModel) {
        guard composerExpanded, model.agentWorking else { return }
        // A composer the reader JUST opened: its focus lands a runloop later,
        // so the field still reads as unfocused — never fold it under them.
        guard !composerFocusPending else { return }
        guard model.draftEditor.isEditing == false else { return }
        guard !attach.busy, !model.hasContent else { return }
        guard composerMenu(model) == .none, slashConfirm == nil else { return }
        withAnimation(motion.standard) { composerExpanded = false }
    }

    // MARK: - Host

    /// Everything an `AgentMarkdownText` needs to render embedded images —
    /// agent prose can carry `![](/api/attachments/{id})` and those fetches are
    /// authenticated (EXP-440).
    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            // EXP-760: narration, messages, plans and question prompts chip
            // issue identifiers the agent names — `#EXP-1` AND the bare
            // `EXP-1` agents actually write — scoped to the session's team.
            issueRefs: AgentIssueRefContext(
                teamId: session.teamId,
                db: deps.db,
                onOpen: { [threadIssueOutlet] issueId in
                    openIssueInnermost(issueId, outlet: threadIssueOutlet, deps: deps, accountId: accountId)
                }
            )
        )
    }

    /// EXP-550: the host machine is asleep and the session is parked on it —
    /// the screen says paused (grey), not "Connecting…" forever.
    private var hostPaused: Bool {
        guard model?.hostDeviceOffline == true else { return false }
        switch model?.phase {
        case .starting, .connecting, .idle, .none: return true
        case let .closed(_, reconnecting): return reconnecting
        default: return false
        }
    }

    /// EXP-549: the CURRENT machine label off the synced devices row.
    private var hostLabel: String {
        model?.hostDevice.displayLabel ?? SessionHostDevice(label: nil, online: nil).displayLabel
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

    // MARK: - Feed

    @ViewBuilder
    private func feedBody(_ model: AgentSessionModel) -> some View {
        if model.feed.isEmpty, let status = historyStatus(model) {
            // EXP-773: a finished session's transcript lives on the machine
            // that ran it, and the relay is asking that machine for it. Say
            // which machine, and say plainly when it can't answer.
            centeredState {
                if model.history == .pending {
                    InlineSpinner()
                }
                Text(status)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .multilineTextAlignment(.center)
            }
        } else if model.feed.isEmpty, hostPaused {
            // EXP-550: the machine hosting this session is asleep. The session
            // is NOT over — it resumes when the machine comes back.
            hostOfflineState
                .frame(maxHeight: .infinity)
        } else if model.feed.isEmpty,
                  model.phase == .connecting || model.phase == .starting || model.phase == .idle {
            // EXP-1275 (D8): a first load is skeleton rows, never a spinner.
            SkeletonRows(rows: 3, rowHeight: 28, dividers: false)
                .transcriptColumn()
                .frame(maxHeight: .infinity, alignment: .top)
                .padding(.top, 8)
        } else if model.feed.isEmpty, model.phase == .live, model.latestDiff == nil {
            centeredState {
                Text("Waiting for activity…")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
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
                .padding(.horizontal, AgentSessionLayout.gutter)
                .padding(.vertical, 6)
            }
            // EXP-927 §2c "Alignment": the tabs read in the transcript's
            // column — the gutter rides inside the scrolling content.
            .transcriptColumnWidth()
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
                InlineSpinner()
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
    /// user scrolls up, then the round "Jump to bottom" button
    /// (`JumpToBottomButton`, EXP-1191) re-pins.
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
                                detail: focused.detail,
                                durationMs: model.subagentDurationMs(
                                    focused.subagentId, done: focused.done
                                ),
                                live: model.agentWorking
                            )
                            if focused.items.isEmpty {
                                EmptyState(.transcript)
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
                                        row: row, context: markdownContext, nested: false,
                                        live: model.agentWorking, testResults: model.testResults
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
                            // THE ask dock: the card waiting on this
                            // steerer leaves the transcript for the
                            // composer's slot (`bottomBand`).
                            let docked = dockedRow(model)?.id
                            let rows = model.rows.filter { $0.id != docked }
                            // EXP-1280: the tool-posted cards NOT in the dock
                            // draw inline — each after the call that posted
                            // it, the ones older than the window up top — so
                            // a tool plan waiting beside a relay question,
                            // and every answered one, stays in the history
                            // (web `renderToolCards`).
                            let toolCardAnchors = AskDock.anchorToolCards(
                                feed: model.feed, cardIds: toolAsks.map(\.id)
                            )
                            let dockedToolIds = dockedToolCardIds(model)
                            inlineToolCards(toolCardAnchors.unanchored, docked: dockedToolIds, model: model)
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
                                inlineToolCards(
                                    Self.rowItemIds(row).flatMap { toolCardAnchors.anchors[$0] ?? [] },
                                    docked: dockedToolIds,
                                    model: model
                                )
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
                                    agent: model.session?.cli ?? session.cli,
                                    caption: { model.workingCaption(now: $0) }
                                )
                                    .padding(.top, Self.gapPoints(AgentFeed.transcriptGap(
                                        prev: rows.last?.rowClass, cur: .tool
                                    )))
                            }
                        }
                        // EXP-1321/1324: THE Queue closes the transcript —
                        // the task list, background work and open waits —
                        // right above the queued-messages strip.
                        if agentTab == nil {
                            TaskQueueView(
                                todoOpen: Bindable(model).queueTodoOpen,
                                completedOpen: Bindable(model).queueCompletedOpen,
                                build: { model.taskQueue(now: $0) }
                            )
                            .padding(.top, DesignTokens.Transcript.gapBlock)
                        }
                        Color.clear
                            .frame(height: 1)
                            .id(AgentSessionLayout.bottomAnchor)
                    }
                    .padding(.vertical, 8)
                    // EXP-787: a transcript is a reading measure. The 16pt
                    // screen inset stays, but the column caps at the shared
                    // `maxWidth` and centres inside whatever is wider — a
                    // landscape phone or an iPad must not stretch prose edge
                    // to edge. EXP-927: the tab row and the strips above the
                    // composer take the same column.
                    .transcriptColumn()
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
                // EXP-1162: no `stickyHeaderFade` — the Work screen's header
                // band hangs the top edge strip over every face itself.
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
                .onChange(of: bottomBandHeight) { old, new in
                    guard atBottom, new != old else { return }
                    // After the inset lands (one runloop hop), and again once
                    // the band's animation and the keyboard have settled.
                    for delay in [0.0, 0.05, DesignTokens.Motion.Duration.standard + 0.2] {
                        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
                            guard atBottom else { return }
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
                    // EXP-1191: the shared 32pt round jump button, centred
                    // 12pt above the bottom band's top edge (the overlay
                    // sits inside the band's safe-area inset). Only while
                    // scrolled away from the newest row of a non-empty feed.
                    ZStack {
                        if !atBottom, !model.feed.isEmpty {
                            JumpToBottomButton {
                                // Re-arm follow directly (Android parity: the
                                // tap sets follow=true) instead of waiting
                                // for the scroll geometry to flip atBottom —
                                // while the agent streams, the animated
                                // scrollTo targets the bottom as of the tap,
                                // the content keeps growing underneath it, and
                                // the animation lands short of the moving max
                                // offset, so the button never vanished
                                // (EXP-306). With atBottom set here the button
                                // hides at once and the growth observer keeps
                                // chasing the bottom.
                                atBottom = true
                                withAnimation(motion.standard) {
                                    proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                                }
                            }
                            .jumpToBottomTransition(motion)
                            .padding(.bottom, JumpToBottomButton.bottomGap)
                            .accessibilityIdentifier("agent-jump-to-bottom")
                        }
                    }
                    .animation(motion.standard, value: atBottom)
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
                refs: markdownContext.issueRefs,
                testResults: model?.testResults ?? [:]
            )
        case let .edits(items):
            // EXP-916: a run of edit calls is ONE card — the same file cards
            // the Changes face draws, stacked flush, the live one open.
            EditedFilesCard(items: items, liveItemId: liveToolRowId)
        case let .expRun(items):
            // EXP-948: a run of calls to the SAME Exponential tool folds into
            // one contract-captioned row of our own ("Read 3 issues") instead
            // of hiding inside a generic "N other tools".
            ExpToolGroupRow(items: items, refs: markdownContext.issueRefs)
        case let .subagentRun(run):
            // EXP-1324: the subagent Trace line + its duration badge; a tap
            // opens the subagent's own tab.
            VStack(alignment: .leading, spacing: 4) {
                SubagentTraceLine(
                    label: SubagentTraceLine.label(run.agentType),
                    running: !run.done && model?.phase == .live,
                    durationMs: model?.subagentDurationMs(run.subagentId, done: run.done),
                    onOpen: { openAgentTab(run.subagentId) }
                )
                // EXP-856: a second copy of this agent — amber, verbatim.
                if let duplicate = run.duplicateDetail {
                    AgentDuplicateWarningRow(detail: duplicate)
                }
            }
        case let .ask(group):
            askCard(group)
        case let .single(item):
            switch item {
            case let .narration(_, text, _, _):
                NarrationBubble(text: text, context: markdownContext)
            case let .apiError(_, message, errorType, _):
                ApiErrorRow(message: message, errorType: errorType)
            case let .tool(
                id, name, detail, _, callId, kind, settled, failed, diff, preview, output, script,
                settledAt
            ):
                toolOrWorkflowRow(
                    item: item, name: name, kind: kind, detail: detail, callId: callId,
                    settled: settled, failed: failed, diff: diff,
                    preview: preview, output: output, live: id == liveToolRowId,
                    script: script, settledAt: settledAt
                )
            case let .userMessage(_, text, _):
                // EXP-724: a steered slash command is a control action, not
                // prose — it renders as a compact pill instead of a bubble.
                // NEW-2: the contract catalog alone (agent commands hidden).
                if let command = SlashCommands.command(
                    for: text,
                    agent: model?.catalogAgent
                ) {
                    CommandPill(command: command, text: text)
                } else {
                    UserMessageBubble(text: text, context: markdownContext)
                }
            case let .question(question):
                questionCard(question)
            case let .subagent(_, _, agentType, status, detail, _, title, _):
                // A `started` whose `completed` never came reads settled
                // once the turn is over (the Reasoning line's rule).
                SubagentRow(
                    agentType: agentType, title: title, status: status, detail: detail,
                    live: model?.agentWorking == true
                )
            case let .permission(_, tool, detail):
                PermissionRow(tool: tool, detail: detail)
            case .compaction:
                CompactionMarkerRow()
            case let .thought(_, _, running, durationMs, _):
                // EXP-1324: the Reasoning line. A `started` edge whose `ended`
                // never came reads settled once the turn is over.
                TraceLine(
                    Trace.reasoning(
                        running: running && model?.agentWorking == true,
                        durationMs: durationMs
                    ),
                    size: .sm
                )
                .frame(maxWidth: .infinity, alignment: .leading)
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
        kind: String?,
        detail: String?,
        callId: String?,
        settled: Bool,
        failed: Bool,
        diff: String?,
        preview: AgentToolPreview?,
        output: String?,
        live: Bool,
        script: AgentScriptLog?,
        settledAt: Int?
    ) -> some View {
        if let workflow: AgentWorkflow = model?.workflow(for: callId) {
            // EXP-1240: THE Work card; a lane opens its agent's tab.
            WorkflowWorkCardRow(
                workflow: workflow,
                runFor: workflowAgentRun,
                onOpenAgent: openAgentTab
            )
        } else if ScriptCards.isScriptCall(item) {
            // EXP-1279: a code-mode call is its Script card.
            ScriptCallRow(settled: settled, failed: failed, script: script)
        } else if settled, !failed, isCommentCall(name) {
            // EXP-1241: a SETTLED comment call is the comment itself; running
            // or failed it keeps its ordinary row.
            CommentCallRow(
                detail: detail, preview: preview, settledAt: settledAt,
                agent: model?.session?.cli ?? session.cli,
                ownerName: runOwnerName,
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
                refs: markdownContext.issueRefs, output: output, live: live,
                kind: kind,
                testResults: callId.flatMap { model?.testResults[$0] }
            )
        }
    }

    /// EXP-895: the ONE row still RUNNING — the last feed item while it is an
    /// unsettled tool call, and only in a LIVE run (an ended run's trailing
    /// unsettled row is history, not a tail). EXP-1206: it is its headline
    /// alone. `AgentFeed`'s rule, shared ×4.
    private var liveToolRowId: Int? {
        guard model?.phase == .live, let feed = model?.feed else { return nil }
        return AgentFeed.liveToolRowId(feed)
    }

    /// The nested run behind one workflow agent — a method rather than a
    /// closure literal at the call site, for the same budget reason.
    private func workflowAgentRun(_ agentId: String?) -> AgentSubagentRun? {
        model?.subagentRun(agentId: agentId)
    }

    /// EXP-1240: a Work card's lane (or a subagent card) opens that agent's
    /// own conversation tab — the transcript's existing nested view.
    private func openAgentTab(_ subagentId: String) {
        agentTab = subagentId
    }

    /// EXP-1241: an `exponential_comments_create` call.
    private func isCommentCall(_ name: String) -> Bool {
        ExpToolDisplay.resolve(toolName: name)?.row == CommentCallRow.tool
    }

    /// EXP-1241: whose key posted a comment call — the session's starter,
    /// named when that is the viewer.
    private var runOwnerName: String? {
        let row = model?.session ?? session
        guard row.userId == deps.auth.userId else { return nil }
        return deps.auth.userName
    }

    /// THE ask dock (fixtures/ask-dock.json): the ONE card waiting on this
    /// steerer (the NEWEST still-active question / ask, as web, desktop and
    /// Android dock it) — what leaves the transcript for the composer's
    /// slot. nil while nothing waits or this client may not answer (a
    /// read-only viewer keeps it inline).
    private func dockedRow(_ model: AgentSessionModel) -> AgentFeedRow? {
        guard model.cardPending, canAnswer else { return nil }
        let active = model.activeQuestionIds
        return model.rows.last { row in
            switch row {
            case let .ask(group): group.questions.contains { active.contains($0.id) }
            case let .single(item): item.question.map { active.contains($0.id) } ?? false
            default: false
            }
        }
    }

    /// EXP-1280: the tool-posted cards still waiting on this viewer (one who
    /// may post), oldest first.
    private func pendingToolAsks(_ model: AgentSessionModel) -> [AskDock.Pending] {
        guard model.mayPost, !model.sessionEnded else { return [] }
        return AskDock.threadPendingAsks(toolAsks) { _ in nil }
    }

    /// EXP-1280: the tool-posted cards THE ask dock holds right now — the
    /// waiting ones, only while no relay ask is docked (web
    /// `dockedToolCards`). Every other tool card draws inline.
    private func dockedToolCardIds(_ model: AgentSessionModel) -> Set<String> {
        guard dockedRow(model) == nil else { return [] }
        return Set(pendingToolAsks(model).map(\.id))
    }

    /// The feed item ids a transcript row draws (web `rowItemIds`): where a
    /// tool-posted card anchors.
    private static func rowItemIds(_ row: AgentFeedRow) -> [Int] {
        switch row {
        case let .single(item): [item.id]
        case let .toolRun(items), let .edits(items), let .expRun(items): items.map(\.id)
        case .subagentRun, .ask: []
        }
    }

    /// EXP-1280: the tool-posted cards with these message ids, inline as
    /// THE thread card (`ThreadAskCard`): a waiting one answerable by a
    /// viewer who may post, an answered one closed with its picks. The
    /// docked ones are skipped (they sit in the composer's slot).
    @ViewBuilder
    private func inlineToolCards(_ ids: [String], docked: Set<String>, model: AgentSessionModel) -> some View {
        let canAnswerCards = model.mayPost && !model.sessionEnded
        ForEach(ids.filter { !docked.contains($0) }, id: \.self) { id in
            if let row = toolAsks.first(where: { $0.id == id }) {
                ThreadAskCard(
                    row: row,
                    canAnswer: canAnswerCards,
                    answeringId: toolAnsweringId,
                    markdownContext: markdownContext,
                    onSubmit: submitToolAnswer
                )
                .padding(.top, DesignTokens.Transcript.gapBlock)
                .accessibilityIdentifier("session-tool-card")
            }
        }
    }

    /// EXP-1280: observe this session's own synced card messages; keep the
    /// tool-posted ones (no `questionId`) as rows.
    private func observeToolAsks() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let id = session.id
        let observation = ValueObservation.tracking { db in
            try SessionQueries.messages(db: db, sessionId: id)
        }
        do {
            for try await messages in observation.values(in: pool) {
                toolAsks = messages.compactMap { row in
                    let message = TreeMessage(messageRow: row)
                    guard message.authorKind == "agent", message.card?.questionId == nil else { return nil }
                    return ThreadRow.askCard(message)
                }
            }
        } catch {}
    }

    /// EXP-1280: answer a tool-posted card (`sessions.answer`); a refusal
    /// toasts and the card stays answerable.
    private func submitToolAnswer(_ card: ThreadCardAnswer) {
        guard toolAnsweringId == nil else { return }
        toolAnsweringId = card.messageId
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await api.answer(
                    accountId: accountId, messageId: card.messageId, answer: card.answer, decision: card.decision,
                    fresh: card.fresh, feedback: card.feedback, answers: card.answers, surfaceData: card.surfaceData
                )
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(
                        verb: "answer", thing: card.decision == nil ? "the question" : "the plan"
                    ),
                    description: error.userFacingMessage
                )
            }
            toolAnsweringId = nil
        }
    }

    /// THE ask dock in the composer's slot: the session view feeds it its
    /// one answerable card (no caption), drawn as THE big card with its
    /// body capped. EXP-1280: with no relay ask docked, the session's own
    /// tool-posted cards (THE thread card, `ThreadAskCard`).
    @ViewBuilder
    private func askDock(_ model: AgentSessionModel) -> some View {
        if dockedRow(model) == nil, case let toolPending = pendingToolAsks(model), !toolPending.isEmpty {
            AskDockView(pending: toolPending) { id, onBack, cap in
                if let row = toolAsks.first(where: { $0.id == id }) {
                    ThreadAskCard(
                        row: row,
                        canAnswer: true,
                        answeringId: toolAnsweringId,
                        markdownContext: markdownContext,
                        onBack: onBack,
                        bodyMaxHeight: cap,
                        onSubmit: submitToolAnswer
                    )
                }
            }
            .padding(.vertical, 8)
            .transcriptColumn()
        } else if let row = dockedRow(model) {
            let question: AgentQuestion? = switch row {
            case let .ask(group): group.questions.first
            case let .single(item): item.question
            default: nil
            }
            if let question {
                AskDockView(pending: [AskDock.Pending(
                    id: String(row.id),
                    kind: question.planMode ? .plan : .question,
                    text: question.text,
                    sessionName: nil,
                    createdAt: 0
                )]) { _, _, cap in
                    switch row {
                    case let .ask(group): askCard(group, bodyMaxHeight: cap)
                    case let .single(item): item.question.map { questionCard($0, bodyMaxHeight: cap) }
                    default: EmptyView()
                    }
                }
                .padding(.vertical, 8)
                .transcriptColumn()
            }
        }
    }

    /// A lone question card: a plan approval or a single-question ask.
    @ViewBuilder
    private func questionCard(_ raw: AgentQuestion, bodyMaxHeight: CGFloat? = nil) -> some View {
        // EXP-1316: every question card offers the free-text answer.
        let question = raw.withFreeTextRow()
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
            markdownContext: markdownContext,
            bodyMaxHeight: bodyMaxHeight
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
    private func askCard(_ group: AgentAskGroup, bodyMaxHeight: CGFloat? = nil) -> some View {
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
            ? editingSteps[group.askId].flatMap { id in answered.first { $0.wireId == id } }?.withFreeTextRow()
            : nil
        if group.questions.indices.contains(index) {
            // EXP-1316: every step offers the free-text answer.
            let current = group.questions[index].withFreeTextRow()
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
                    markdownContext: markdownContext,
                    bodyMaxHeight: bodyMaxHeight
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
                    markdownContext: markdownContext,
                    bodyMaxHeight: bodyMaxHeight
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
        return total > 1 ? "\(index) of \(total)" : nil
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

    /// EXP-804: the usage wall — the title has no caption line, so it is a
    /// banner here.
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
        // EXP-1031: the transient items — a failed Stop or send — are toasts
        // (`withLifecycle`'s `onChange`s); only the standing states stay
        // banners.
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
                    InlineSpinner()
                }
                Text(detail ?? (reconnecting ? "Connection lost. Reconnecting…" : "Disconnected"))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        case .starting where !model.feed.isEmpty:
            bannerRow {
                InlineSpinner()
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
    private var hostOfflineState: some View {
        EmptyState(
            title: "\(hostLabel) is offline",
            description: "The agent is paused on that machine and continues when it comes back online.",
            icon: AppIcons.uiDeviceOffline
        )
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
            .glassRow(isOpaque: true)
            // The transcript's reading column (EXP-927 §2c), like the strips.
            .transcriptColumn()
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(AgentFeed.compactingLabel)
        }
    }

    // MARK: - Rate-limit banner (EXP-784)

    /// The agent's rate-limit window, in the compaction strip's slot: its own
    /// message and how long until the window resets. It stands only while
    /// the slot holds a WALL (EXP-818: a warning over a working run is not
    /// one) whose reset has not passed (EXP-831: it re-reads the model's 30s
    /// activity clock and drops itself a minute past the stamp) — an
    /// `ok`/empty status, the replay swap and the end of the run all clear it.
    @ViewBuilder
    private func rateLimitBanner(_ model: AgentSessionModel) -> some View {
        if let limit = model.sessionRateLimit, !model.isOver,
           AgentFeed.rateLimitBannerShows(limit, now: model.activityNow) {
            let caption = AgentFeed.rateLimitCaption(limit, now: model.activityNow)
            // Web `RateLimitBanner` is the reference: ONE row, 6pt gaps,
            // centered — the usage glyph + the message in the semantic
            // yellow (one truncating line), the relative countdown as a
            // separate muted caption.
            HStack(alignment: .center, spacing: 6) {
                HStack(alignment: .center, spacing: 6) {
                    AppIcon(AppIcons.uiUsage, size: AppIcon.Size.small)
                        .foregroundStyle(DesignTokens.Semantic.yellow)
                    Text(verbatim: caption.message)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.yellow)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Text(verbatim: AgentFeed.rateLimitDetail(caption.countdown))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .fixedSize(horizontal: true, vertical: false)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(
                    AgentFeed.rateLimitAccessibilityLabel(limit, now: model.activityNow)
                )
                Spacer(minLength: 0)
                // EXP-849: the wall's PRIMARY move is another login — only
                // for whoever may steer the session.
                if model.mayPost {
                    accountSwitchControl(model)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow(isOpaque: true)
            // The transcript's reading column (EXP-927 §2c), like the strips.
            .transcriptColumn()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-rate-limit")
        }
    }

    /// EXP-849: "Switch account" — THE account picker the composer uses,
    /// narrowed to the session's agent on its host machine; a pick sends
    /// `sessions.send({sessionId, account})` and the host resumes the SAME
    /// session on that login between turns (web `SessionAccountSwitchButton`).
    /// A refused switch keeps its control and says why when tapped.
    @ViewBuilder
    private func accountSwitchControl(_ model: AgentSessionModel) -> some View {
        let blocked = model.accountSwitchBlocked
        let options = model.accountSwitchOptions
        let pill = GlassPill(
            SessionAccountSwitch.wallSwitchLabel,
            icon: AppIcons.uiSwap,
            mode: .action { showAccountSwitch = true },
            primary: true,
            enabled: blocked == nil && !model.switchingAccount
        )
        .accessibilityHint(blocked ?? SessionAccountSwitch.costNote)
        .accessibilityIdentifier("session-account-switch")
        if let blocked {
            pill.overlay(
                Color.clear
                    .contentShape(Capsule())
                    .onTapGesture { toaster.error(blocked) }
            )
        } else {
            pill.background(
                AccountPicker(
                    options: options.map(\.account),
                    value: options.first(where: \.current)?.account.key,
                    onChange: { key in
                        guard let picked = options.first(where: { $0.account.key == key }) else { return }
                        if let refusal = picked.blockedReason {
                            toaster.error(refusal)
                            return
                        }
                        Task { await model.switchAccount(to: picked.account) }
                    },
                    mark: { AgentBrandMark.image($0) },
                    open: $showAccountSwitch,
                    hideTrigger: true,
                    disabledKeys: Set(options.filter { $0.blockedReason != nil }.map(\.account.key)),
                    title: SessionAccountSwitch.wallSwitchLabel,
                    trigger: { EmptyView() }
                )
            )
        }
    }

    private func bannerRow(@ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 8) {
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        // EXP-1161: the band floats over the feed, so the banner wears the
        // strips' opaque row instead of sitting bare on the transcript.
        .glassRow(isOpaque: true)
        // The same reading column as the transcript above it.
        .transcriptColumn()
    }

    // MARK: - Bottom bar (steering input)

    /// EXP-820: while a question or plan card is pending on this steerer
    /// (`cardPending`: live, not ended, an unresolved card in the feed) the
    /// composer band is GONE — the card's free answer is its own inline
    /// field, so the composer would only compete with it. The draft stays in
    /// the model and the band is back once the card resolves. EXP-621: the
    /// composer is tied to the SESSION, not the socket — it stays up through
    /// a reconnect and only goes away once the session is over. EXP-1295: a
    /// viewer who may not post (a private session) never gets one.
    private func bandRetired(_ model: AgentSessionModel) -> Bool {
        model.isOver || model.cardPending || !model.mayPost || !pendingToolAsks(model).isEmpty
    }

    @ViewBuilder
    private func bottomBar(_ model: AgentSessionModel) -> some View {
        if bandRetired(model) {
            EmptyView()
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
                // EXP-1316: THE floating composer (ExpUI), shared with a
                // thread's Conversation.
                FloatingComposer(
                    placeholder: model.composerPlaceholder,
                    open: composerOpen(model),
                    collapsedLabel: "Message the agent",
                    collapsedIdentifier: "agent-composer-collapsed",
                    onExpand: expandComposer
                ) {
                    if hasUsage {
                        usageRingCircle(model)
                    }
                } card: {
                    composerCard(model)
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
            || !model.pendingFiles.isEmpty
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
        // Holds the idle fold off until the focus below lands (or the retry
        // window has passed — a field that never takes focus must not pin
        // the composer open forever).
        composerFocusPending = true
        // Programmatic focus needs the field mounted — one runloop hop, with
        // a 150ms retry in case the first lands before layout. EXP-802: the
        // same shape as IssueDetailBottomBar's, driven off the editor model.
        DispatchQueue.main.async { focusComposer() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
            if composerExpanded, model?.draftEditor.isEditing == false { focusComposer() }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            composerFocusPending = false
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

    /// `sessions.markRead` on this session; a background failure stays silent.
    private func markRead() {
        let api = deps.sessionsApi
        let accountId = accountId
        let sessionId = session.id
        Task { try? await api.markRead(accountId: accountId, sessionId: sessionId) }
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

    // MARK: - Composer card

    /// EXP-1295: THE session composer box (`SessionComposerCard`) with the
    /// session's own controls: the plan chip, the model pill, the usage ring
    /// (EXP-893), the `/` keys (EXP-724) and Stop in the send slot.
    private func composerCard(_ model: AgentSessionModel) -> some View {
        // EXP-790: an EMPTY field while the agent works offers Stop (the
        // interrupt) in the send slot; the first typed character brings Send
        // back.
        let showsStop = model.agentWorking && !model.hasContent
        return SessionComposerCard(
            draft: model,
            // EXP-820: the generic prompt — a pending card's free answer is
            // the card's own inline field.
            placeholder: model.composerPlaceholder,
            controls: SessionComposerFooter.Controls(
                planModeActive: model.planModeActive,
                modelValue: model.composerModelValue(),
                agent: model.catalogAgent,
                onPickModel: { alias in sendModelSwitch(model, alias) },
                usageFraction: usageFraction(model),
                usageSeverity: usageSeverity(model),
                onUsage: { showUsageSheet = true },
                showsPlan: model.session?.dispatch != true,
                // An ended row takes no `/model`: the pin is a label.
                ended: model.session?.isLive == false,
                planModeFallback: model.sessionConfig == nil && model.session?.planMode == true,
                // EXP-1252: the Device pin before Model.
                devicePin: model.devicePin
            ),
            attach: $attach,
            // EXP-1295: a message is a `messages` row, so sending never waits
            // for the socket — only for a send already in flight.
            sendEnabled: model.canSend,
            showsStop: showsStop,
            canStop: model.canStop,
            onStop: { model.stop() },
            onReturn: { handleComposerReturn(model) },
            onSend: { sendMessage(model) },
            onIssueRefTap: { issueId in
                openIssueInnermost(issueId, outlet: threadIssueOutlet, deps: deps, accountId: accountId)
            },
            onArrow: { delta in
                guard composerMenu(model) == .slash else { return false }
                moveSlashHighlight(delta, model)
                return true
            },
            onEscape: {
                guard composerMenu(model) == .slash else { return false }
                slashDismissedFor = model.draftText
                return true
            }
        )
        .onChange(of: model.draftText) { _, _ in
            slashHighlight = 0
        }
    }

    /// EXP-877: a model switch is a PLAIN MESSAGE — `/model <alias>` — the
    /// agent answers with the next `config_state`, which repaints the pill.
    private func sendModelSwitch(_ model: AgentSessionModel, _ alias: String) {
        guard model.session?.isLive != false else { return }
        Task { await model.sendMessage("/model \(alias)") }
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
        // A slash command is the WHOLE message: no attachments around it
        // (the toast refuses, web parity; `/btw` included), and EXP-724:
        // `/clear` discards the whole conversation — ask first, then send
        // exactly what was typed.
        if let command = SlashCommands.command(
            for: model.draftText, agent: model.catalogAgent
        ) {
            if model.hasPendingAttachments {
                toaster.error(SlashCommands.attachmentsRefusal)
                return
            }
            if command.confirm {
                slashConfirm = command
                return
            }
        }
        Task { await model.sendDraft() }
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
struct NarrationBubble: View {
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
struct UserMessageBubble: View {
    let text: String
    let context: AgentMarkdownContext
    /// EXP-1245: the turn bubble's muted caption (`<name> · <time>`), under
    /// the bubble on the trailing edge; nil in the transcript.
    var caption: String?
    /// EXP-1317: a person's OWN queued message — the `Remove from queue` X
    /// at the END of the caption line (`sessions.unqueue`); nil = no X.
    var onRemove: (() -> Void)? = nil

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
        // Wave D: trailing file lines split off exactly like the embeds.
        let hasImages = !parsed.attachmentIds.isEmpty || !parsed.files.isEmpty
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
                if !parsed.attachmentIds.isEmpty {
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
                // Wave D: each file line = the comment thread's file row
                // (glyph + name, opens the attachment), never a broken image.
                if !parsed.files.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(Array(parsed.files.enumerated()), id: \.offset) { _, file in
                            SteerFileLinkRow(file: file)
                        }
                    }
                    .padding(.top, parsed.text.isEmpty && parsed.attachmentIds.isEmpty ? 0 : 4)
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
        .modifier(BubbleCaption(caption: caption, onRemove: onRemove))
    }
}

/// EXP-1245: the turn bubble's caption, muted under it on the trailing edge.
/// EXP-1317 (×4): a queued row's caption ends on the `Remove from queue` X.
private struct BubbleCaption: ViewModifier {
    let caption: String?
    var onRemove: (() -> Void)?

    func body(content: Content) -> some View {
        if (caption?.isEmpty == false) || onRemove != nil {
            VStack(alignment: .trailing, spacing: 4) {
                content
                HStack(spacing: 4) {
                    if let caption, !caption.isEmpty {
                        Text(caption)
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .accessibilityIdentifier("user-message-caption")
                    }
                    if let onRemove {
                        Button(action: onRemove) {
                            AppIcon(AppIcons.uiClose, size: 10, weight: .semibold)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .frame(width: 22, height: 22)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(AgentFeed.queueRemoveLabel)
                        .accessibilityIdentifier("thread-queue-remove")
                    }
                }
                .padding(.trailing, onRemove == nil ? 4 : 0)
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
        } else {
            content
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
struct QuestionCard: View {
    let question: AgentQuestion
    /// "2 of 3" for a step of a multi-question ask — the muted caption beside
    /// the heading (web `AskCard` `meta`); nil for a lone card.
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
    /// THE ask dock (fixtures/ask-dock.json): the asking session's name, a
    /// muted truncating caption beside the label (a thread's card).
    var caption: String? = nil
    /// THE ask dock: a card focused out of the stack leads its header row
    /// with the back arrow (`ui-back`) that returns to the stack.
    var onBack: (() -> Void)? = nil
    /// THE ask dock: the body's cap (`bodyMaxHeightFraction` of the screen);
    /// the body scrolls inside it instead of folding. nil = in the
    /// transcript (the "Show more" fold).
    var bodyMaxHeight: CGFloat? = nil
    /// EXP-1280 (plan-card.json): a rich card's form under the body — the
    /// inline surface and the question blocks (`PlanCardForm.swift`); shown
    /// open and answered alike (answered = inert, the picks highlighted).
    var form: AnyView? = nil
    /// EXP-1280: why the plan cannot be approved yet (`copy.answerIssues`);
    /// the approve rows wait and the line shows under them. Rejecting never
    /// needs answers.
    var approveIssue: String? = nil
    /// EXP-1280: the form answers the card itself (a question's picture
    /// tiles): the option rows stay hidden.
    var hidesOptions: Bool = false
    /// EXP-1280: the answered plan's feedback, under its decision (ONE
    /// `copy.feedbackLine`).
    var resolutionNote: String? = nil
    /// EXP-1280: the form shows the answer itself (a thread question card's
    /// closed picks): no resolution line.
    var hidesResolution: Bool = false
    /// EXP-1280: the card's inline pictures (the rich card: radius 12 + the
    /// 1pt hairline, no max height) vs the transcript's.
    var richBody: Bool = false
    /// EXP-1280: the gap between the body and the form (a plan's blocks /
    /// a surface = `blockGap`, a question card's options = `optionsGap`).
    var formGap: CGFloat = PlanCard.Card.blockGap
    /// EXP-1280 (×4): the pending plan's feedback field, ALWAYS below the
    /// decision rows (`AskDock.Copy.feedbackPlaceholder`); it rides any
    /// decision as `feedback`, so the reject row no longer expands.
    var feedback: Binding<String>? = nil

    @State private var expanded = false
    @State private var dockedBodyHeight: CGFloat = 0
    /// Tap order is the submit order of the semantic answer frame.
    @State private var picked: [String] = []
    /// EXP-820: the option whose inline field is open — a free-text row
    /// ("Type something.") or the plan's reject row. Tapping the row again
    /// folds it.
    @State private var inlineKey: String? = nil
    @State private var inlineText = ""
    @FocusState private var inlineFocused: Bool

    /// The gap between the card's parts (web `mt-2`).
    private static let contentSpacing: CGFloat = 8
    private static let clampChars = 600
    private static let clampLines = 6
    private static let clampHeight: CGFloat = 160
    /// EXP-788: how many rows get a numbered chip — the desktop's keystrokes
    /// are single digits.
    private static let numberedRows = 9

    /// Plans are always fully rendered — never folded (EXP-197); a docked
    /// body scrolls in its cap instead.
    private var clampable: Bool {
        bodyMaxHeight == nil && !question.planMode
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
    private var rejectKey: String? { feedback == nil ? AgentFeed.planRejectKey(for: question) : nil }

    /// EXP-820: a row that expands into an inline field instead of answering
    /// on the tap — a free-text row, or the plan's reject row.
    private func expandsInline(_ option: AgentQuestionOption) -> Bool {
        option.freeText || option.key == rejectKey
    }

    /// The tinted heading line (web `AskCard` `label`): "Plan ready" for a
    /// plan, the step's header inside a stepper (else "Question" / "Review
    /// answers"), a lone question's header when it has one.
    private var headerText: String? {
        if question.planMode { return AskDock.Copy.planReady }
        if stepLabel != nil || question.isSubmitStep {
            if let header = question.header, !header.isEmpty { return header }
            return question.isSubmitStep ? "Review answers" : "Question"
        }
        if let header = question.header, !header.isEmpty { return header }
        return nil
    }

    /// EXP-1191: the card's one tint — a plan's primary, a question's yellow.
    private var tint: Color {
        question.planMode ? DesignTokens.Palette.primary : DesignTokens.Semantic.yellow
    }

    /// Multi-select answers batch into one submit; everything else answers on
    /// the option tap.
    private var needsExplicitSubmit: Bool { question.multiSelect && !hidesOptions }

    private var submitTitle: String { AgentFeed.submitLabel }

    var body: some View {
        // EXP-1191 (web `AskCard` parity): ONE top-aligned row — the 14pt
        // glyph beside a column holding the heading, the body and the
        // options, so the options indent under the text, never the glyph.
        HStack(alignment: .top, spacing: 8) {
            // THE ask dock: the back arrow LEADS the header row (web
            // `AskCard` `onBack`: ghost icon-xs, `-my-1 -ml-1`).
            if let onBack {
                AskDockBackButton(action: onBack)
            }
            // EXP-820: no blue on the card — a plan's glyph and header are the
            // primary, a question's the semantic yellow.
            AppIcon(question.planMode ? AppIcons.codingPlan : AppIcons.uiHelp, size: 14)
                .foregroundStyle(tint)
                // Onto the first text line: the 12pt heading, else the
                // larger prompt body.
                .padding(.top, hasHeader ? 1 : 4)
            VStack(alignment: .leading, spacing: 0) {
                if hasHeader {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        if let headerText {
                            Text(headerText)
                                .font(.caption.weight(.medium))
                                .foregroundStyle(tint)
                                .lineLimit(1)
                                .truncationMode(.tail)
                        }
                        if let stepLabel {
                            Text(stepLabel)
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .fixedSize()
                        }
                        if let caption, !caption.isEmpty {
                            Text(caption)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                                .truncationMode(.tail)
                        }
                    }
                    .padding(.bottom, 4)
                }
                cardContent
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        // A bordered card, not a group container: the ask is one free-content
        // block that has to stand off the transcript behind it. Docked, it
        // floats over the scrolling transcript, so it is opaque.
        .glassCard(isOpaque: bodyMaxHeight != nil)
        .onAppear(perform: preselectEditingAnswer)
    }

    private var hasHeader: Bool {
        headerText != nil || stepLabel != nil || caption?.isEmpty == false
    }

    /// Everything under the heading: answered steps, the body, then the
    /// options 8pt below it (web `mt-2`).
    @ViewBuilder
    private var cardContent: some View {
            VStack(alignment: .leading, spacing: Self.contentSpacing) {
                priorStepSummary
                prompt
                if showsResolution {
                    if !hidesResolution {
                        resolution
                    }
                } else {
                    if !hidesOptions {
                        optionList
                    }
                    // EXP-1280 (×4): the decision rows, THEN the issue line,
                    // THEN the feedback field.
                    if let approveIssue, answerable {
                        Text(approveIssue)
                            .font(.caption2)
                            .foregroundStyle(DesignTokens.Semantic.yellow)
                            .accessibilityIdentifier(PlanCard.TestIds.issue)
                    }
                    if let feedback, answerable {
                        PlanCardField(
                            placeholder: AskDock.Copy.feedbackPlaceholder,
                            text: feedback,
                            multiline: true,
                            identifier: PlanCard.TestIds.feedback
                        )
                        .disabled(locked)
                    }
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
            Text(answer ?? (step.dismissed ? "Dismissed" : PlanCard.Copy.answered))
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

    /// The body + (EXP-1280) the rich card's form. THE ask dock: they never
    /// grow past the cap together, they scroll inside it; the decision rows
    /// stay below.
    @ViewBuilder
    private var prompt: some View {
        if let bodyMaxHeight {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    markdownBody
                    formBlock
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { dockedBodyHeight = $0 }
            }
            .scrollBounceBehavior(.basedOnSize)
            .frame(height: min(max(dockedBodyHeight, 1), bodyMaxHeight))
        } else if form != nil {
            VStack(alignment: .leading, spacing: 0) {
                foldablePrompt
                formBlock
            }
        } else {
            foldablePrompt
        }
    }

    /// The rich card's form (surface + question blocks), `formGap` under
    /// the body.
    @ViewBuilder
    private var formBlock: some View {
        if let form {
            form.padding(.top, formGap)
        }
    }

    private var markdownBody: some View {
        AgentMarkdownText(
            text: question.text,
            context: markdownContext,
            options: [.autolinkBareURLs],
            imageMaxHeight: richBody ? nil : 280,
            imageCornerRadius: richBody ? PlanCard.Card.pictureRadius : 8,
            imageStroke: richBody ? PlanCard.Card.pictureStroke : nil
        )
    }

    @ViewBuilder
    private var foldablePrompt: some View {
        let clamped = clampable && !expanded
        // No hard line breaks: a prompt or plan body is authored GFM prose,
        // where a wrapped source line is one paragraph.
        markdownBody
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
                    ?? (question.dismissed ? "Dismissed" : PlanCard.Copy.answered)
            )
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
                .textSelection(.enabled)
        }
        if let resolutionNote, !resolutionNote.isEmpty {
            Text(PlanCard.Copy.feedbackLine(resolutionNote))
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .textSelection(.enabled)
                .accessibilityIdentifier(PlanCard.TestIds.feedback)
        }
    }

    /// EXP-1280: an approve row of a plan whose questions still wait.
    private func approveBlocked(_ option: AgentQuestionOption) -> Bool {
        guard question.planMode, approveIssue != nil else { return false }
        return AskDock.planOption(key: option.key)?.decision == PlanDecision.approved.rawValue
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
        VStack(alignment: .leading, spacing: 4) {
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
                        .disabled(locked || approveBlocked(option))
                        .opacity(locked || approveBlocked(option) ? 0.5 : 1)
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
            // ONE in-flight caption ×4 (`copy.answering`), sent or confirmed:
            // the card stays locked until the answer is stamped / the
            // desktop retires it with `question_resolved`.
            HStack(spacing: 6) {
                InlineSpinner()
                Text(PlanCard.Copy.answering)
                    .font(.caption)
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
        // Web parity: the Button's row centers its checkbox, text and keycap.
        let row = HStack(alignment: .center, spacing: 8) {
            if let checked {
                // Multi-select rows carry an explicit checkbox (EXP-529) —
                // the glassRow tint alone was too subtle to read the picked
                // set off the card. Android parity (QuestionOptionLabel).
                AppIcon(checked ? AppIcons.uiSelected : AppIcons.uiUnselected, size: 13)
                    .foregroundStyle(
                        .white.opacity(checked ? TextOpacity.primary : TextOpacity.tertiary)
                    )
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
            // EXP-820: a row that types unfolds; EXP-1316: the free-text row
            // looks like every other option (number badge only, ×4) — only
            // the plan's reject row keeps the chevron.
            if expandable, !option.freeText {
                AppIcon(inlineOpen ? AppIcons.uiChevronUp : AppIcons.uiChevronDown, size: 11)
                    .foregroundStyle(quietColor)
            }
            if checked == nil, number <= Self.numberedRows {
                // EXP-1191 (web parity): the number key sits at the row's
                // TRAILING end as a small keycap chip.
                numberChip(number, primary: primary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .frame(minHeight: 32)
        .contentShape(Rectangle())
        let shape = RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
        let active = selected || inlineOpen
        return Group {
            if primary {
                row.background(DesignTokens.Palette.primary, in: shape)
            } else {
                // The web's outline option: a row fill under the card
                // hairline; a pick (or an open field) the active glass.
                row
                    .background(active ? GlassTokens.fillActive : GlassTokens.fillRow, in: shape)
                    .overlay(
                        shape.stroke(
                            active ? GlassTokens.strokeActive : GlassTokens.strokeCard,
                            lineWidth: GlassTokens.hairline
                        )
                    )
            }
        }
    }

    /// The 1..9 keycap — the keystroke the desktop would take (web `kbd`
    /// parity, EXP-1191): a 16pt-tall outlined chip, monospaced 10pt, muted;
    /// on the primary row its border is the primary foreground at 30%.
    private func numberChip(_ number: Int, primary: Bool) -> some View {
        Text("\(number)")
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(
                primary
                    ? DesignTokens.Palette.primaryForeground.opacity(0.8)
                    : .white.opacity(TextOpacity.tertiary)
            )
            .padding(.horizontal, 4)
            .frame(minWidth: 16, minHeight: 16, maxHeight: 16)
            .overlay(
                RoundedRectangle(cornerRadius: 4)
                    .stroke(
                        primary
                            ? DesignTokens.Palette.primaryForeground.opacity(0.3)
                            : GlassTokens.strokeCard,
                        lineWidth: 1
                    )
            )
            .accessibilityHidden(true)
    }
}

/// EXP-1324: a tool call as its Trace line (`Trace.tool`, size sm): "Ran
/// bun test", "Reading README.md", another tool's name + detail. A settled
/// call that PRINTED something (an `execute`'s output) is a Trace collapsible
/// whose content is the Output CodeBlock (`ansi` for a command, destructive
/// when the call failed); the RUNNING call is its line alone (EXP-1206: the
/// transcript never jumps while a command prints). A settled command whose
/// output parses as a test run with a failure IS the failed-tests line.
private struct ToolRow: View {
    let name: String
    let detail: String?
    /// EXP-785: the call failed — the line tints destructive.
    var failed: Bool = false
    /// EXP-787: a row INSIDE an expanded group (kept for the call sites; the
    /// Trace's own `lineGap` spaces nested lines now).
    var nested: Bool = false
    /// EXP-846: the call SETTLED — a `tool_update` status landed.
    var settled: Bool = false
    /// EXP-846: the result an Exponential MCP call distilled. Nil otherwise.
    var preview: AgentToolPreview? = nil
    /// EXP-846: the run's team + where a tapped issue preview goes.
    var refs: AgentIssueRefContext? = nil
    /// EXP-895: what an `execute` call PRINTED (redacted, tail-cut).
    var output: String? = nil
    /// EXP-895: this call is the ONE still running (`AgentFeed.liveToolRowId`).
    var live: Bool = false
    /// The contract `toolKind` (nil = `other`).
    var kind: String? = nil
    /// A settled `execute`'s output parsed as a test run — ONCE, when its
    /// settle landed (`AgentSessionModel.testResults`), never per render.
    var testResults: TestResults? = nil

    var body: some View {
        // EXP-846: one of OUR OWN MCP tools keeps its own sentence + mark (as
        // a Trace line); a FAILED call is the generic destructive line.
        if !failed, let display = ExpToolDisplay.resolve(toolName: name) {
            ExpToolRow(
                display: display,
                subject: detail,
                settled: settled,
                preview: preview,
                refs: refs
            )
        } else {
            genericRow
        }
    }

    private var line: TraceLineModel {
        Trace.tool(
            toolKind: kind ?? "other", name: name, detail: detail,
            settled: settled,
            failed: failed
        )
    }

    /// Anything to disclose — never while the call still runs (EXP-1206).
    private var shownOutput: String? {
        guard let output, !live, !output.isEmpty else { return nil }
        return output
    }

    private var failedTests: TraceLineModel? {
        guard settled, kind == "execute", shownOutput != nil else { return nil }
        return Trace.failedTests(testResults)
    }

    @ViewBuilder
    private var genericRow: some View {
        TraceStack(size: .sm) {
            // A settled test run with failures IS its failed-tests line
            // (trace.json), opening onto the same output — it replaces the
            // call's own "Ran …" line, as on web, desktop and Android.
            if let failedTests, let shownOutput {
                TraceCollapsible(size: .sm, accessibilityId: "agent-tool-row") {
                    TraceLine(failedTests, size: .sm)
                } content: {
                    CodeBlockView(shownOutput, language: .ansi, destructive: true)
                }
            } else if let shownOutput {
                TraceCollapsible(size: .sm, accessibilityId: "agent-tool-row") {
                    TraceLine(line, size: .sm)
                } content: {
                    CodeBlockView(
                        shownOutput,
                        language: kind == "execute" ? .ansi : .plain,
                        destructive: failed
                    )
                }
            } else {
                TraceLine(line, size: .sm)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
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
///   session/board/action          → a name chip
///   results                       → an `Open Results` button (EXP-933)
///   picture                       → the shown picture as ONE tile (EXP-1172)
///   none                          → the caption alone
///
/// EXP-920: a preview that carries `refs` (what the answer touched, one
/// entity each) draws `EntityRefChips` instead — one chip per ref, a tap
/// opens its preview sheet — and the kinds above only serve older publishers.
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
    @Environment(\.sessionResultsRaw) private var sessionResultsRaw

    /// EXP-1172: a settled `sessions_show` call's picture, by the attachment
    /// id its answer carried — nil while the upload is still in flight.
    private var inlinePicture: SessionResultEntry? {
        guard settled, display.result == .picture, let id = preview?.id else { return nil }
        return sessionResultPicture(sessionResultsRaw, attachmentId: id)
    }

    private var trimmedSubject: String? {
        guard let subject else { return nil }
        let text = subject.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            // EXP-1324: the contract's sentence + our mark, as a Trace line
            // (the subject is its muted description).
            TraceLine(
                title: display.caption(settled: settled),
                description: trimmedSubject,
                status: settled ? nil : .running,
                size: .sm
            ) {
                ExpLogoMark(size: Trace.Geometry.sm.icon)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            // Only a SETTLED call has a result; a preview that arrived early
            // (the publisher only sends one with a status) waits for it.
            if settled, let preview {
                previewRow(preview)
            }
            // EXP-1172: a settled `sessions_show` call draws its picture as
            // ONE tile under the row, once it is in the synced results.
            if let inlinePicture {
                SessionInlinePicture(entry: inlinePicture)
                    .padding(.top, 4)
            }
        }
    }

    @ViewBuilder
    private func previewRow(_ preview: AgentToolPreview) -> some View {
        // EXP-920: a publisher that named what the answer touched gets one
        // chip per entity (a tap opens its preview); a pre-EXP-920 preview
        // keeps the single-subject rendering below, untouched.
        if !preview.refs.isEmpty {
            EntityRefChips(refs: preview.refs, context: refs)
        } else {
            legacyPreviewRow(preview)
        }
    }

    @ViewBuilder
    private func legacyPreviewRow(_ preview: AgentToolPreview) -> some View {
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
        case .results, .picture:
            // The `Open Results` button / the inline tile below the row IS
            // its preview (EXP-933 / EXP-1172).
            EmptyView()
        case .session, .board, .action, .comment, .prompt, .agent, .event:
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
    /// The model's parsed test runs by call id (`AgentSessionModel.testResults`).
    var testResults: [String: TestResults] = [:]

    @State private var expanded = false

    /// EXP-785: the collapsed caption is what the calls DID ("Ran 3 commands
    /// · edited 2 files"), the one summary every client derives from the
    /// contract fixture, not a bare count.
    private var caption: String {
        ToolGroupSummary.summarize(items.compactMap { item in
            guard case let .tool(_, _, detail, _, _, kind, _, failed, _, _, _, _, _) = item
            else { return nil }
            return ToolCallSummary(kind: kind ?? "other", detail: detail, failed: failed)
        })
    }

    /// EXP-1324: the group is a Trace collapsible: the tool glyph + the
    /// contract caption; open, its calls are Trace lines on the rail.
    private var groupFailed: Bool {
        items.contains { item in
            if case let .tool(_, _, _, _, _, _, _, failed, _, _, _, _, _) = item { return failed }
            return false
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            TraceCollapsible(size: .sm, open: $expanded, accessibilityId: "agent-tool-group") {
                TraceLine(
                    TraceLineModel(icon: Trace.Icons.tool, title: caption, description: nil, status: nil),
                    size: .sm
                )
            } content: {
                ForEach(items) { item in
                    if case let .tool(
                        id, name, detail, _, callId, kind, settled, failed, _, preview, output, _, _
                    ) = item {
                        // EXP-1206: inside the group too, the RUNNING call
                        // is its line alone.
                        ToolRow(
                            name: name, detail: detail, failed: failed,
                            nested: true,
                            settled: settled, preview: preview, refs: refs,
                            output: output, live: liveTail && id == items.last?.id,
                            kind: kind,
                            testResults: callId.flatMap { testResults[$0] }
                        )
                    }
                }
            }
            // The live tail: while the run is the trailing row of a live
            // session, the latest call shows under the collapsed caption.
            if !expanded, liveTail, let last = items.last,
               case let .tool(_, name, detail, _, callId, kind, settled, failed, _, preview, output, _, _) = last {
                ToolRow(
                    name: name, detail: detail, failed: failed, nested: true,
                    settled: settled, preview: preview, refs: refs,
                    output: output, live: true, kind: kind,
                    testResults: callId.flatMap { testResults[$0] }
                )
                .padding(.leading, Trace.Geometry.sm.contentInset)
                .padding(.top, Trace.Geometry.sm.contentTop)
            }
        }
    }
}

/// EXP-948 — a run of consecutive calls to the SAME Exponential MCP tool,
/// collapsed into ONE row of our own: the Exponential mark every single call of
/// ours already wears, plus the contract's plural caption ("Reading 3 issues" →
/// "Read 3 issues · 1 failed"), expandable to the individual rows.
///
/// Our own calls never join a generic "N other tools" fold (`AgentFeed`'s
/// projection breaks a tool run on them), so this is the only place a pile of
/// them can collapse — and it says WHAT they did.
private struct ExpToolGroupRow: View {
    let items: [AgentFeedItem]
    /// EXP-846: what an Exponential call's issue preview resolves against (the
    /// run's team) and where a tap on it goes. Nil keeps the preview textual.
    var refs: AgentIssueRefContext? = nil

    @State private var expanded = false
    @Environment(\.sessionResultsRaw) private var sessionResultsRaw

    /// The contract's own sentence — `ExpToolGroup.caption`, locked ×4 by the
    /// shared fixture.
    private var caption: String { ExpToolGroup.caption(items) }

    /// EXP-1172: a run of `sessions_show` calls keeps its pictures visible
    /// while FOLDED — each settled, non-failed call's picture (by its
    /// `preview.id`) in call order; a picture never hides in a fold. Expanded,
    /// the single rows carry their own tiles, so this strip is empty.
    private var foldedPictures: [SessionResultEntry] {
        guard !expanded else { return [] }
        return items.compactMap { item in
            guard case let .tool(_, name, _, _, _, _, settled, failed, _, preview, _, _, _) = item,
                  settled, !failed,
                  ExpToolDisplay.resolve(toolName: name)?.result == .picture
            else { return nil }
            return sessionResultPicture(sessionResultsRaw, attachmentId: preview?.id)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // EXP-1324: a Trace collapsible — our mark + the contract's
            // plural sentence; open, the calls are Trace lines on the rail.
            TraceCollapsible(size: .sm, open: $expanded, accessibilityId: "agent-exp-tool-group") {
                TraceLine(title: caption, size: .sm) {
                    ExpLogoMark(size: Trace.Geometry.sm.icon)
                }
            } content: {
                ForEach(items) { item in
                    if case let .tool(
                        _, name, detail, _, _, kind, settled, failed, _, preview, output, _, _
                    ) = item {
                        ToolRow(
                            name: name, detail: detail, failed: failed,
                            nested: true,
                            settled: settled, preview: preview, refs: refs,
                            output: output, kind: kind
                        )
                    }
                }
            }
            let pictures = foldedPictures
            if !pictures.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(pictures.enumerated()), id: \.offset) { _, picture in
                        SessionInlinePicture(entry: picture)
                    }
                }
                .padding(.top, 8)
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
    /// The run is working a turn: a Reasoning line shimmers only then (a
    /// `started` whose `ended` never came reads settled once the turn ends).
    var live: Bool = true
    /// The model's parsed test runs by call id (`AgentSessionModel.testResults`).
    var testResults: [String: TestResults] = [:]

    var body: some View {
        switch row {
        case let .edits(items):
            // A digest inside someone else's transcript: no row of it is the
            // live one — the subagent's own tab is where its work is read.
            EditedFilesCard(items: items)
                .padding(.vertical, nested ? 2 : 0)
        case let .expRun(items):
            // EXP-948: the lane folds its own Exponential runs the same way.
            ExpToolGroupRow(items: items, refs: context.issueRefs)
                .padding(.vertical, nested ? 2 : 0)
        case let .single(item):
            SubagentItemRow(
                item: item, context: context, nested: nested, live: live, testResults: testResults
            )
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
    /// The run is working a turn (see `SubagentLaneRow.live`).
    var live: Bool = true
    var testResults: [String: TestResults] = [:]

    var body: some View {
        content.padding(.vertical, nested ? 2 : 0)
    }

    @ViewBuilder
    private var content: some View {
        switch item {
        case let .tool(_, name, _, _, _, kind, settled, failed, _, _, _, script, _)
            where ScriptCards.isScriptCall(name: name, toolKind: kind):
            // EXP-1279: a lane's code-mode call is its Script card too.
            ScriptCallRow(settled: settled, failed: failed, script: script)
        case let .tool(_, name, detail, _, callId, kind, settled, failed, _, preview, output, _, _):
            // A subagent's rows are a digest inside the main transcript — its
            // own tab is where its work is read — so none of them is the live
            // row here; the reader's tap still opens one.
            ToolRow(
                name: name, detail: detail, failed: failed,
                settled: settled, preview: preview, refs: context.issueRefs,
                output: output, kind: kind,
                testResults: callId.flatMap { testResults[$0] }
            )
        case let .thought(_, _, running, durationMs, _):
            // The main lane's rule: shimmer only while the run works.
            TraceLine(Trace.reasoning(running: running && live, durationMs: durationMs), size: .sm)
        case let .narration(_, text, _, _):
            NarrationBubble(text: text, context: context)
        case let .apiError(_, message, errorType, _):
            ApiErrorRow(message: message, errorType: errorType)
        case let .userMessage(_, text, _):
            UserMessageBubble(text: text, context: context)
        default:
            EmptyView()
        }
    }
}

/// A stray subagent marker (EXP-249) — a `completed` whose `started` fell off
/// the top of the feed, or a start with nothing published under it yet; also
/// the focused tab's header. EXP-1324: the subagent Trace line.
private struct SubagentRow: View {
    let agentType: String
    /// EXP-847: the spawn's own description, when it named one.
    var title: String? = nil
    let status: AgentSubagentStatus
    let detail: String?
    var durationMs: Int? = nil
    /// The run is working a turn: an unfinished subagent shimmers only then.
    var live: Bool = true

    var body: some View {
        SubagentTraceLine(
            label: SubagentTraceLine.label(agentType),
            running: status != .completed && live, durationMs: durationMs
        )
    }
}

/// EXP-1324: THE subagent line — "Running the Explore agent" → "Ran the
/// Explore agent" + the duration as the outline Badge.
struct SubagentTraceLine: View {
    let label: String
    let running: Bool
    var durationMs: Int? = nil
    /// Tap = open the subagent's own tab (the line then carries a chevron).
    var onOpen: (() -> Void)? = nil

    /// The line names the agent TYPE ("Ran the Explore agent", web parity);
    /// the wire's fallback type reads as the unnamed "Ran a subagent".
    static func label(_ agentType: String) -> String {
        let type = agentType.trimmingCharacters(in: .whitespaces)
        return type == AgentFeed.subagentFallbackType ? "" : type
    }

    var body: some View {
        let model = Trace.subagent(
            label: label, running: running, failed: false, durationMs: durationMs
        )
        let line = TraceLine(
            title: model.title, status: model.status, size: .sm,
            icon: { AppIcon(TraceIcons.glyph(model.icon), size: Trace.Geometry.sm.icon) },
            accessory: {
                if let badge = model.description { TraceBadge(badge, size: .sm) }
            }
        )
        if let onOpen {
            Button(action: onOpen) {
                HStack(spacing: Trace.Geometry.sm.chevronGap) {
                    line
                    AppIcon(AppIcons.traceChevron, size: Trace.Geometry.sm.icon)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("agent-subagent-line")
        } else {
            line.frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// A permission prompt the agent hit (EXP-249) — INFORMATIONAL only: the
/// approval lives in the desktop's own TUI, there is nothing to answer here.
/// A transient API error the agent hit mid-turn (`api_error`): a quiet
/// inline row — never the rate-limit banner, never a wall. The message is
/// plain text (it is the CLI's own words, not markdown).
private struct ApiErrorRow: View {
    let message: String
    let errorType: String?

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.uiWarning, size: 11)
                .foregroundStyle(DesignTokens.Palette.destructive.opacity(0.7))
                .padding(.top, 2)
            Text(verbatim: message)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let errorType, !errorType.isEmpty {
                Text(verbatim: errorType.replacingOccurrences(of: "_", with: " "))
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.top, 1)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

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
/// EXP-1316 (1n): the thread Conversation follows with the same tracker.
struct FollowPinTracker: ViewModifier {
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
    ///
    /// EXP-975: `stranded` is the one caller that skips the `atBottom` gate.
    /// A viewport past the content's end shows nothing whoever scrolled
    /// where, so the pin's state is no reason to leave it there; the scroll
    /// it issues lands within slack of the end and the pin metrics re-arm
    /// follow off that movement like any other arrival.
    private func queueGrowthRepin(stranded: Bool = false) {
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
            guard stranded || atBottom, !userScrolling else { return }
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
                    } else if FeedFollowPolicy.stranded(
                        below: Double(new.below), slack: Double(slack), userScrolling: userScrolling
                    ) {
                        // EXP-975: the content SHRANK under the offset — a
                        // lazy estimate collapsing after the first burst of a
                        // fresh run pinned against it, a shorter history
                        // committing, the window sliding a tall row out — and
                        // the viewport now hangs past the end over nothing
                        // but the background. UIKit clamps that offset only
                        // on the next gesture; the reader saw a black screen
                        // until they dragged. Same deferred, rate-limited
                        // scroll as growth, so a strand that keeps re-forming
                        // can no more storm the layout than growth can.
                        queueGrowthRepin(stranded: true)
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
struct FeedBottomOverflowKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

extension AppDependencies {
    /// EXP-1152: the ONE claim on a run's socket owner (EXP-621's
    /// ref-counted `SteerSessionStore.attach`), shared by the Run page and
    /// the Changes page: each holds its own claim while it is up, so a
    /// FINISHED run's model (and its diff) survives whichever page the pager
    /// happens to unmount first. Pair every call with
    /// `steerSessions.detach`.
    @MainActor
    func attachSteerModel(accountId: String, session: SessionEntity) -> AgentSessionModel {
        let model = steerSessions.attach(accountId: accountId, sessionId: session.id) {
            AgentSessionModel(
                accountId: accountId,
                session: session,
                currentUserId: auth.userId,
                steerApi: steerApi,
                sessionsApi: SessionsApi(trpc: trpc),
                attachmentsApi: attachmentsApi,
                issuesApi: issuesApi,
                db: db
            )
        }
        // EXP-909: the overlay's one-shot usage refresh rides the device
        // command queue. Set on every claim — a reattached model was built on
        // an earlier one.
        model.devicesApi = devicesApi
        return model
    }
}


/// EXP-1358: an observation boundary. SwiftUI tracks `@Observable` reads
/// per view BODY; a `@ViewBuilder` closure invoked here is read by THIS
/// body, so the properties it touches re-render this slice alone, never the
/// view that built the closure.
struct BodySlice<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        content()
    }
}
