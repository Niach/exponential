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
    /// Within this many points of the bottom still counts as pinned. #EXP-1383
    /// (owner 2026-10-24: the chats' jump "exactly as in sessions"): the run
    /// view and every chat read the ONE fixture value, chats.json
    /// `transcript.followSlack`.
    static let followSlack = CGFloat(Chats.Transcript.followSlack)
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
/// card in a chat (or any session ref). A chat-style view of the session
/// over the scrubbed steer activity channel (eNet, VAPP-43): narration bubbles, compact tool
/// rows, collapsible subagent runs, question cards. NO terminal rendering.
///
/// The header is the back button, a back row naming the CHAT (its root,
/// EXP-1310) over the session's name (S-3: no Stop; the composer's
/// stop-turn is the one); the composer box sends a `messages` row
/// (`sessions.send({sessionId})`). The steer feed stays the LIVE layer (streaming
/// text, tool lines, usage). Read-only for a viewer who may not post (a
/// teammate's session on a private device).
struct AgentSessionView: View {
    let accountId: String
    /// The row the screen opened on — the model observes it from there on.
    let session: SessionEntity
    /// EXP-1343: a page of the chat screen — no header of its own (the
    /// chat's band + strip are the ONE header).
    var embedded = false
    /// EXP-1343: the claimed model, handed to the host (nil once the page
    /// left), keyed by this session's id.
    var onModel: ((String, AgentSessionModel?) -> Void)? = nil
    /// #EXP-1732 / #FEED-204: an app editor's Chat tab (its root's session
    /// view): the composer "+" Device row; a pick on a live root also moves
    /// it (`moveDevice`). nil = no Device row.
    var editorDevice: SessionComposerFooter.DeviceMenu? = nil
    /// #FEED-204 / #EXP-1732: an app editor's ended root, revived on another
    /// device: the next send is `sessions.send({appId, deviceId})`.
    var editorMove: ChatComposerDraft.AppMove? = nil
    /// #EXP-1729: an app editor's Account and Model rows (an ended root's
    /// = the launch picks riding its next send; a live root's are its own,
    /// `liveAccount` standing in when its switch offers no row).
    var editorPicks: ChatEditorPicks? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL
    @Environment(\.toaster) private var toaster
    @Environment(\.pushRoute) private var pushRoute
    /// EXP-1347: inside a chat, an issue that is one of its tabs opens
    /// that tab (the innermost outlet).
    @Environment(\.chatIssueOutlet) private var chatIssueOutlet
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
    /// #EXP-1703: the offline banner's Move picker is open.
    @State private var showDeviceMove = false
    /// Whether the feed is scrolled to (within slack of) its bottom —
    /// auto-scroll only while pinned; scrolling up pauses follow and surfaces
    /// the "Jump to bottom" pill.
    @State private var atBottom = true
    /// steer-enet.json REPLAY TAIL: the row that headed the transcript when
    /// the reader's scroll near its top paged older rows in; the scroll
    /// lands back on it once they are above it (nil = no page pending).
    @State private var earlierAnchor: Int?
    /// #EXP-1380: this chat's message limit as last read (`Chats.MessageLimits`).
    @State private var messageLimit = 0
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
    /// NEW-2: the team's prompts, listed after the builtins in the `/` menu.
    @State private var slashPrompts = TeamSlashPrompts()
    /// EXP-790: the composer is a folded capsule until it is tapped, and
    /// folds again on blur when nothing would be lost (IssueDetailBottomBar's
    /// rule). A non-empty draft or a pending image keeps it open regardless.
    @State private var composerExpanded = false
    /// EXP-1316: the bottom band's measured height (#EXP-1782: where the
    /// feed's scroll area ends) — a pinned feed re-pins as it changes.
    @State private var bottomBandHeight: CGFloat = 0
    /// Set by `expandComposer`, cleared once the editor takes focus (or the
    /// retry window passes): the EXP-931 idle fold stands down meanwhile.
    @State private var composerFocusPending = false
    /// EXP-820: per ask id, the wire id of the EARLIER step being re-answered
    /// ("go back" in the stepper), or absent while the current step shows.
    /// View state, not model state: it is a place in the card, and a fresh
    /// screen starts on the current step.
    @State private var editingSteps: [String: String] = [:]
    /// EXP-1280: this session's OWN tool-posted cards (`exponential_plan_propose`
    /// / `exponential_sessions_ask`: a `plan` / `question` message with no
    /// mirrored ACP ask) — the relay has no open ask for them, so they answer
    /// through `sessions.answer`, in the ask dock like a chat's.
    @State private var toolAsks: [SessionChatRow] = []
    /// EXP-1398: this session's cards (`AskDock.sessionCards`: the tool-posted
    /// plan / question cards above plus its `surface` cards), oldest first.
    @State private var sessionCards: [AskDock.SessionCard] = []
    /// #EXP-1562: the chat header's waiting pick and the card it lit.
    @Environment(\.waitingFocus) private var waitingFocus
    @State private var highlightId: String?
    /// waiting-cards.json DEEP LINKS: THE pending deep link into a card
    /// (`DeepLinkScroll`), the pick it came from, its row's frame.
    @State private var deepLink: WaitingCards.DeepLink?
    @State private var deepLinkTaken: WaitingFocus?
    @State private var deepLinkFrame: CGRect?
    /// The session's cards have been read once (a link waits for them).
    @State private var cardsLoaded = false
    /// EXP-1398: the `surface` cards' rows, by message id.
    @State private var surfaceRows: [String: SessionChatRow] = [:]
    /// EXP-1323 / #EXP-1732: a ROOT's suggested tasks rows (`buildSessionChat`
    /// over its tree): a row with an open item rides THE Dock for a viewer
    /// who may post, otherwise it draws inline.
    @State private var suggestionRows: [SessionChatRow] = []
    /// In-flight suggestion actions (`ChatSuggestionActions.key`).
    @State private var suggestionPending: Set<String> = []
    /// #EXP-1715: the Teams pick (nil = every team), seeded from the root's
    /// synced `scope_team_ids` and re-seeded when another client moves it.
    @State private var teamsPicked: [String]?
    /// #EXP-1793: the team's MCP servers for an agent chat's "+" MCP servers
    /// page (`mcpServers.list`).
    @State private var mcpServers: [McpServerRow] = []
    /// #EXP-1714: optional, a specimen may mount the view without one.
    @Environment(TeamState.self) private var teamState: TeamState?
    /// #EXP-1736: this session's tree snapshot (the senders' names and
    /// avatars, transcript-grouping.json) and its S-3 `origin` (the session
    /// that started it; nil = a person did).
    @State private var transcriptRows: ChatRows?
    @State private var originSessionId: String?
    /// #VAPP-229 (via-app.json): the synced, live app this session's
    /// `viaAppId` resolves to; nil = no "via <app>" line.
    @State private var viaAppName: String?
    @State private var toolAnsweringId: String?
    /// N-12: THE Dock's open sheet (a card id).
    @State private var dockOpenId: String?
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
            // the chat's Guide (the ROOT's `guide`).
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

    /// #VAPP-229 (via-app.json): the app whose tool started this session,
    /// pinned over the transcript (web `InPlaceSession`'s header, desktop's):
    /// it never scrolls away. #EXP-1736: who started it is no header any
    /// more: its prompt is that session's bubble.
    @ViewBuilder
    private var sessionLinks: some View {
        if let caption = viaAppCaption(viaAppName) {
            Text(caption)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityIdentifier("session-via-app")
                .transcriptColumn()
                // Web's `pt-3 pb-1`.
                .padding(.top, 12)
                .padding(.bottom, 4)
        }
    }

    /// The transcript and the composer band UNDER it (#EXP-1782,
    /// fixtures/dock.json): the feed's scroll view ENDS at the band's top
    /// edge (the composer's when no card waits) and clips there; nothing
    /// scrolls under or behind the band, no edge fade. The band's measured
    /// height re-pins a pinned feed when it changes, and the "Jump to
    /// bottom" circle rides 12pt above its top.
    @ViewBuilder
    private func runFace(_ model: AgentSessionModel) -> some View {
        // #EXP-1623: the transcript is its own observation scope too, so a
        // feed write re-renders the transcript, never this view's chrome,
        // header and modifiers around it.
        VStack(spacing: 0) {
            sessionLinks
            BodySlice {
                feedBody(model)
            }
            .frame(maxHeight: .infinity)
            .clipped()
            // EXP-1358: the band is its OWN observation scope. Built
            // inline, every draft read it makes (`trimmedDraft`,
            // `slashMatches`, `hasContent`) was tracked by THIS view's
            // body, so each keystroke re-rendered the whole transcript.
            BodySlice {
                bottomBand(model)
            }
            // EXP-1316: the band's measured height is where the feed's
            // scroll area ends; a pinned feed re-pins when it changes
            // (the composer expanding, a card landing, late layout).
            .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { height in
                bottomBandHeight = height
            }
        }
    }

    /// N-12: THE Dock on the feed's bottom edge (fixtures/dock.json): the
    /// waiting cards (the relay's own ask, the tool-posted ones), THE Queue,
    /// the queued messages and the status banners as DockCards, then the
    /// quick replies and THE composer (always: it never yields its slot).
    /// The cards are opaque; #EXP-1782: the band sits under the feed, no
    /// edge strip, fade or shadow.
    private func bottomBand(_ model: AgentSessionModel) -> some View {
        let rows = dockRows(model)
        return DockView(
            rows: rows,
            openId: $dockOpenId,
            renderSheet: { row, close in dockSheet(row, model: model, close: close) },
            renderAction: { row in dockAction(row, model: model) }
        ) {
            // EXP-1211: the agent's quick replies, directly above the
            // composer; gone whenever the composer is.
            // Web's `/` menu floats over them: they yield while it is up.
            if !bandRetired(model), composerMenu(model) != .slash {
                SessionComposerReplies(sessionId: session.id)
                    .environment(\.accountId, accountId)
            }
            bottomBar(model)
        }
    }

    // MARK: - Header

    /// The nav bar: the system back button, a two-line title (EXP-1310, web
    /// `session-header`, Android `SessionHeaderBar`): the back row naming
    /// the CHAT over the session's name (S-3: no Stop).
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
            }
    }

    private var headerTitle: some View {
        VStack(spacing: 1) {
            // The way back to the chat (a chat already on the stack is
            // popped back to — `pushRoute`'s rule).
            if let chatTitle = model?.chatTitle, !chatTitle.isEmpty {
                Button {
                    pushRoute(.chat(accountId: accountId, chatId: session.rootId))
                } label: {
                    HStack(spacing: 2) {
                        AppIcon(AppIcons.uiChevronLeft, size: 11)
                        Text(chatTitle)
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
                                    if let model {
                                        prepareSend(model)
                                        Task { await model.sendDraft() }
                                    }
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
            // #EXP-1380: a widened message limit (Load earlier) re-reads.
            .task(id: "\(session.id)#\(messageLimit)") { await observeTranscriptSenders() }
            .onReceive(NotificationCenter.default.publisher(for: Chats.MessageLimits.changed)) { note in
                let rootId = session.rootId.isEmpty ? session.id : session.rootId
                guard (note.object as? String) == rootId else { return }
                messageLimit = Chats.MessageLimits.shared.limit(rootId)
            }
            .onChange(of: model?.session?.scopeTeamIds ?? session.scopeTeamIds, initial: true) { _, ids in
                teamsPicked = ids
            }
            .task(id: session.viaAppId) { await observeViaApp() }
            .task(id: "\(session.id)|\(model == nil)") { await loadMcpPick() }
            .task(id: session.teamId) {
                slashPrompts.bind(accountId: accountId, teamId: session.teamId, db: deps.db)
            }
            // EXP-1031: the transient outcomes land as toasts.
            .onChange(of: model?.stopError) { _, message in
                guard let message else { return }
                toaster.error(Chats.Copy.StopTurn.failed, description: message)
            }
            .onChange(of: model?.stopNotDeliveredAt) { _, at in
                guard at != nil else { return }
                toaster.warning(Chats.Copy.StopTurn.notDelivered)
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
                // EXP-1323: opening a session view reads it (the Chats
                // list's bold child row).
                markRead()
            }
            // A newer agent message read here is read: re-stamp the session
            // (the Android/Chat rule — keyed on the newest agent message).
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
                onOpen: { [chatIssueOutlet] issueId in
                    openIssueInnermost(issueId, outlet: chatIssueOutlet, deps: deps, accountId: accountId)
                },
                mentionMembers: ChatMentionMembers.members(
                    teamId: session.teamId, db: deps.db, accountId: accountId)
            )
        )
    }

    /// EXP-549: the CURRENT machine label off the synced devices row.
    private var hostLabel: String {
        model?.hostDevice.displayLabel ?? SessionHostDevice(label: nil, online: nil).displayLabel
    }

    /// EXP-773: why an ended run shows no transcript — which machine holds
    /// it. Nil while a device feeds the session. Mirrored ×4.
    private func historyStatus(_ model: AgentSessionModel) -> String? {
        switch model.history {
        case .deviceOffline:
            return "\(hostLabel) is offline. The transcript lives on that machine."
        case nil: return nil
        }
    }

    // MARK: - Feed

    @ViewBuilder
    private func feedBody(_ model: AgentSessionModel) -> some View {
        if let placeholder = noFeedPlaceholder(model) {
            noFeedBody(placeholder, model: model)
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

    /// What a session view with no feed shows instead of its transcript; nil
    /// = the transcript.
    private enum NoFeedPlaceholder {
        case history(String)
        case deviceAway
        case idle
        case loading
        case waitingForActivity
    }

    private func noFeedPlaceholder(_ model: AgentSessionModel) -> NoFeedPlaceholder? {
        guard !model.hasFeed else { return nil }
        if let status = historyStatus(model) { return .history(status) }
        if model.phase != .live,
           SteerNoFeed.state(
               ended: model.sessionEnded, busy: (model.session ?? session).busy,
               deviceOffline: model.hostDevice.offline
           ) == .away {
            return .deviceAway
        }
        if model.phase == .starting,
           SteerNoFeed.state(
               ended: model.sessionEnded, busy: (model.session ?? session).busy
           ) == .idle {
            return .idle
        }
        if model.phase == .connecting || model.phase == .starting || model.phase == .idle {
            return .loading
        }
        if model.phase == .live, model.latestDiff == nil { return .waitingForActivity }
        return nil
    }

    /// #EXP-1785 (waiting-cards.json NO FEED): a placeholder has no transcript
    /// to hold the cards; the session's WAITING ones draw under it, so a
    /// header pick always reaches an answerable card.
    @ViewBuilder
    private func noFeedBody(_ placeholder: NoFeedPlaceholder, model: AgentSessionModel) -> some View {
        let waitingIds = sessionCards.filter(\.waiting).map(\.id)
        if waitingIds.isEmpty {
            noFeedPlaceholderView(placeholder, model: model)
        } else {
            GeometryReader { geo in
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(spacing: 0) {
                            noFeedPlaceholderView(placeholder, model: model)
                            VStack(alignment: .leading, spacing: 0) {
                                inlineToolCards(waitingIds, model: model)
                            }
                            .padding(.vertical, 8)
                            .transcriptColumn()
                        }
                        .frame(minHeight: geo.size.height)
                    }
                    .modifier(DeepLinkScroll(
                        focus: waitingFocus, taken: $deepLinkTaken, link: $deepLink,
                        highlight: $highlightId, targetFrame: deepLinkFrame, proxy: proxy,
                        ready: cardsLoaded, found: { waitingIds.contains($0) }
                    ))
                }
            }
        }
    }

    @ViewBuilder
    private func noFeedPlaceholderView(_ placeholder: NoFeedPlaceholder, model: AgentSessionModel) -> some View {
        switch placeholder {
        case let .history(status):
            // EXP-773: a finished session's transcript lives on the machine
            // that ran it. Say which machine when it can't answer (VAPP-43:
            // no device fed the join).
            centeredState {
                Text(status)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .multilineTextAlignment(.center)
            }
        case .deviceAway:
            // #EXP-1703 (steer-enet.json A DEVICE THAT IS AWAY): a live row
            // whose host is offline says so at once, never "starting"; a
            // cached feed shows as the transcript instead.
            let away = SteerNoFeed.away(deviceName: model.hostDevice.label)
            centeredState {
                EmptyState(title: away.title, description: away.text, icon: AppIcons.uiDeviceOffline)
            }
            .accessibilityIdentifier("transcript-device-away")
        case .idle:
            // EXP-1487 (steer-enet.json NO FEED, EMPTY TRANSCRIPT): NO FEED on
            // a row that is not busy (idle, or on an open question) is never
            // "starting": the idle empty state, while the model keeps redialing.
            centeredState {
                EmptyState(title: SteerNoFeed.idleTitle, description: SteerNoFeed.idleText)
            }
        case .loading:
            // EXP-1275 (D8): a first load is skeleton rows, never a spinner.
            SkeletonRows(rows: 3, rowHeight: 28, dividers: false)
                .transcriptColumn()
                .frame(maxHeight: .infinity, alignment: .top)
                .padding(.top, 8)
        case .waitingForActivity:
            centeredState {
                Text("Waiting for activity…")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
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
                                // #EXP-1736: the tab is a transcript too:
                                // its prose = THE message bubble under the
                                // subagent's avatar, the turns to it own.
                                let lane = Self.laneGrouping(laneRows, run: focused)
                                ForEach(
                                    Array(laneRows.enumerated()), id: \.element.id
                                ) { index, row in
                                    let layout = lane.layouts[row.id]
                                    TranscriptRow(
                                        side: layout?.side ?? .full,
                                        gap: lane.bubbleGaps[row.id] ?? Self.gapPoints(AgentFeed.transcriptGap(
                                            // The SubagentRow header above the
                                            // list is the first "previous row".
                                            prev: index == 0 ? .tool : laneRows[index - 1].rowClass,
                                            cur: row.rowClass
                                        )),
                                        avatar: lane.avatars[row.id]
                                    ) {
                                        SubagentLaneRow(
                                            row: row, context: markdownContext, nested: false,
                                            live: model.agentWorking, testResults: model.testResults,
                                            author: layout?.name == true ? layout?.sender?.displayName : nil
                                        )
                                    }
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
                                TranscriptLoadEarlierButton {
                                    loadEarlierMessages()
                                    model.loadEarlier()
                                }
                            }
                            // S-3 (dock.json): a waiting plan or question
                            // stays inline, answerable, never a DockCard.
                            // #EXP-1625: the model's immutable row snapshots.
                            let entries = model.transcript
                            let lastId = entries.last?.id
                            // EXP-1398 (ask-dock.json `anchorCases`): EVERY
                            // session card draws inline after the call that
                            // posted it (paired per kind), the ones older than
                            // the window up top (web `renderToolCards`).
                            let toolCardAnchors = AskDock.anchorSessionCards(
                                model.feedCalls, cards: sessionCards
                            )
                            inlineToolCards(toolCardAnchors.unanchored, model: model)
                            // #EXP-1736 (transcript-grouping.json): who sent
                            // each row, its group, its avatar.
                            let grouping = feedGrouping(entries, anchors: toolCardAnchors, model: model)
                            // EXP-787: every row's space above it comes from
                            // the shared gap ladder, keyed on the row BEFORE
                            // it — the rows themselves carry no outer margin
                            // any more, so the rhythm is one derivation
                            // instead of a padding per row kind.
                            // #EXP-1625: each row sits behind an equality gate
                            // on its own inputs, so a pass of this list that
                            // changed nothing for a row (a live tick) never
                            // re-evaluates it.
                            ForEach(entries) { entry in
                                let isLast = entry.id == lastId
                                let layout = grouping.layouts[entry.id]
                                // The EXP-787 ladder between rows; the
                                // grouping's 3 / 12 between two bubbles.
                                TranscriptRow(
                                    side: layout?.side ?? .full,
                                    gap: grouping.bubbleGaps[entry.id] ?? Self.gapPoints(entry.gap),
                                    avatar: grouping.avatars[entry.id]
                                ) {
                                    TranscriptRowSlot(key: rowKey(entry, isLast: isLast, layout: layout, model: model)) {
                                        feedRow(entry.row, isLast: isLast, layout: layout)
                                    }
                                    .equatable()
                                }
                                inlineToolCards(
                                    Self.rowItemIds(entry.row).flatMap { toolCardAnchors.anchors[$0] ?? [] },
                                    model: model
                                )
                            }
                            // EXP-1323 / #EXP-1732: the root's suggested
                            // tasks with nothing open left (or for a reader).
                            inlineSuggestionCards(model)
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
                                        prev: entries.last?.row.rowClass, cur: .tool
                                    )))
                            }
                            // #FEED-237 (message-delivery.json ORDER): the
                            // viewer's own messages the device has not
                            // consumed, then the outbox, each with its mark.
                            ownPendingBubbles(model)
                        }
                        // N-12: THE Queue left the transcript for THE Dock
                        // (`SessionDock`).
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
                // Large chats: the first layout starts at the bottom (the
                // initial offset only, never re-anchored on size changes,
                // EXP-70), so the opening pass sizes the newest rows, not the
                // oldest before the scroll-to-bottom.
                .modifier(InitialBottomOffset())
                .modifier(FollowPinTracker(
                    atBottom: $atBottom,
                    slack: AgentSessionLayout.followSlack,
                    hold: deepLink != nil,
                    repin: { proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom) }
                ))
                // steer-enet.json REPLAY TAIL: within one viewport of the
                // top, older rows page in by themselves (the same action as
                // Load earlier, no new UI); the reader stays on the row they
                // were reading once the page lands above it.
                .modifier(LoadEarlierNearTop(
                    // A pending deep link pages for itself, one page at a time.
                    enabled: agentTab == nil && !atBottom && model.canLoadEarlier && deepLink == nil,
                    load: {
                        let anchor = model.transcript.first?.id
                        loadEarlierMessages()
                        if model.loadEarlier(), let anchor { earlierAnchor = anchor }
                    }
                ))
                .onChange(of: model.transcript.first?.id) { _, first in
                    guard let anchor = earlierAnchor, first != anchor else { return }
                    earlierAnchor = nil
                    Task { @MainActor in proxy.scrollTo(anchor, anchor: .top) }
                }
                // waiting-cards.json DEEP LINKS: a jump into a card (a
                // Needs you row, a waiting pick) pages older rows in until
                // the card is loaded, centres it, then lights it; the follow
                // pin stays off while it is pending.
                .modifier(DeepLinkScroll(
                    focus: waitingFocus, taken: $deepLinkTaken, link: $deepLink,
                    highlight: $highlightId, targetFrame: deepLinkFrame, proxy: proxy,
                    ready: cardsLoaded, found: { drawsCard($0, model: model) },
                    more: model.canLoadEarlier || transcriptRows?.messagesMore == true,
                    arrival: "\(model.transcript.first?.id ?? -1)|\(transcriptRows?.messages.count ?? -1)",
                    page: {
                        let more = transcriptRows?.messagesMore == true
                        loadEarlierMessages()
                        return model.loadEarlier() || more
                    },
                    onTake: {
                        atBottom = false
                        agentTab = nil
                    },
                    onMissing: {
                        atBottom = true
                        proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                    }
                ))
                .onAppear {
                    if deepLink == nil {
                        proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                    }
                    // Lazy rows can still be sizing on the first pass, landing
                    // the scroll short — re-assert once layout has settled.
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(50))
                        if atBottom, deepLink == nil {
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
                .onChange(of: model.feedCount) { _, _ in
                    if atBottom {
                        proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                    }
                }
                // #FEED-237: a press puts the bubble at the bottom: follow it.
                .onChange(of: model.ownPending.count) { old, new in
                    guard new > old else { return }
                    atBottom = true
                    proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                }
                // Switching conversation tabs re-pins to the newest event
                // (EXP-356).
                // EXP-795: back at the bottom, the window slides with the stream
                // again (web/Android/desktop parity).
                .onChange(of: atBottom) { _, now in
                    if now { earlierAnchor = nil }
                    model.noteAtBottom(now)
                }
                .onChange(of: agentTab) { _, _ in
                    // A deep link taken closes the tab to reach its card.
                    guard deepLink == nil else { return }
                    atBottom = true
                    proxy.scrollTo(AgentSessionLayout.bottomAnchor, anchor: .bottom)
                }
                .overlay(alignment: .bottom) {
                    // EXP-1191: the shared 32pt round jump button, centred
                    // 12pt above the bottom band's top edge (#EXP-1782: the
                    // feed's scroll view ends at that edge). Only while
                    // scrolled away from the newest row of a non-empty feed.
                    ZStack {
                        if !atBottom, model.hasFeed {
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
                                deepLink = nil
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

    /// #EXP-1625: everything `feedRow` draws one row from, as a value. The
    /// rows that read live or per-view state beyond it (cards, a running
    /// subagent's clock, a workflow's Work card) are `volatile`: they
    /// re-evaluate with the list, exactly as before.
    private func rowKey(
        _ entry: TranscriptEntry, isLast: Bool, layout: TranscriptGrouping.RowLayout?, model: AgentSessionModel
    ) -> TranscriptRowKey {
        let ids = Self.rowItemIds(entry.row)
        var volatile = false
        var working = false
        var callIds: [String] = []
        switch entry.row {
        case let .toolRun(items), let .edits(items), let .expRun(items):
            callIds = items.compactMap(Self.callId)
        case let .subagentRun(run):
            volatile = !run.done
        case .ask:
            volatile = true
        case let .single(item):
            switch item {
            case .question:
                volatile = true
            case .subagent, .thought, .narration:
                working = model.agentWorking
            case let .tool(_, _, _, _, callId, _, _, _, _, _, _, _, _):
                volatile = model.workflow(for: callId) != nil
                callIds = callId.map { [$0] } ?? []
            default:
                break
            }
        }
        let live = model.phase == .live
        let liveTool = live ? model.openToolRowId.flatMap { ids.contains($0) ? $0 : nil } : nil
        return TranscriptRowKey(
            entry: entry,
            layout: layout,
            liveTail: isLast && live,
            liveTool: liveTool,
            working: working,
            testResults: callIds.map { model.testResults[$0] },
            agent: model.catalogAgent,
            cli: model.session?.cli ?? session.cli,
            volatile: volatile
        )
    }

    @ViewBuilder
    private func feedRow(_ row: AgentFeedRow, isLast: Bool, layout: TranscriptGrouping.RowLayout?) -> some View {
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
                // #EXP-1736: the agent's prose = THE message bubble under its
                // avatar. #VAPP-258: the live tail is the reply being written.
                senderBubble(text, layout: layout, streaming: isLast && model?.agentWorking == true)
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
            case let .userMessage(_, text, _, at):
                // #EXP-1756: the bubble's `HH:mm` (transcript-grouping.json).
                let time = at.map { GlassTimeRow.formatted(Date(timeIntervalSince1970: Double($0) / 1000)) }
                // EXP-724: a steered slash command is a control action, not
                // prose — it renders as a compact pill instead of a bubble.
                // NEW-2: the contract catalog alone (agent commands hidden).
                if case let .marker(_, label) = TranscriptGrouping.feedInput(text) {
                    // #EXP-1772: a system input = THE inline marker (a
                    // hidden one never reaches the transcript).
                    TranscriptMarkerRow(label: label)
                } else if let divider = Chats.commandDivider(text) {
                    // EXP-1373: `/new` and `/compact` = the centred divider.
                    TranscriptDividerPill(
                        icon: AppIcons.conceptOrName(divider.icon) ?? divider.icon,
                        text: divider.label
                    )
                } else if let command = SlashCommands.command(
                    for: text,
                    agent: model?.catalogAgent
                ) {
                    CommandPill(command: command, text: text)
                } else if let input = TranscriptGrouping.crossSessionInput(text) {
                    // #EXP-1736: a parent's or child's message reaching this
                    // session = THAT session's bubble; #EXP-1772: a relay,
                    // its name ` ›` above it the only press, never folded.
                    senderBubble(input.body, layout: layout, streaming: false, time: time)
                } else if layout?.side == .other {
                    // ONE origin rule: the feed's first prompt = the
                    // origin's bubble (the session that started this one).
                    senderBubble(text, layout: layout, streaming: false, time: time)
                } else {
                    // #FEED-237: the live transcript's echo of the turn = two checks.
                    UserMessageBubble(text: text, context: markdownContext, caption: time, status: .read)
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
                TranscriptMarkerRow()
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

    /// #EXP-1736: anyone-but-the-viewer's bubble in the feed: the group's
    /// first one names its sender; a cross-session sender's press pushes
    /// that session.
    private func senderBubble(
        _ text: String,
        layout: TranscriptGrouping.RowLayout?,
        streaming: Bool,
        time: String? = nil
    ) -> some View {
        let author = layout?.name == true ? layout?.sender?.displayName : nil
        let link = layout?.sender?.link
        return MessageBubble(
            side: .other,
            author: author,
            time: time,
            onPress: link.map { sessionId -> () -> Void in { openSender(sessionId) } }
        ) {
            AgentMarkdownText(text: text, context: markdownContext, streaming: streaming, hugsWidth: true)
        }
    }

    private func openSender(_ sessionId: String) {
        pushRoute(
            transcriptRows?.sessionRoute(sessionId, accountId: accountId)
                ?? .agentSession(accountId: accountId, sessionId: sessionId)
        )
    }

    /// #EXP-1736: the feed's grouping — per entry id its layout, the
    /// avatar on a group's first row, and the grouping gap where a bubble
    /// follows a bubble (else the EXP-787 ladder stays).
    struct FeedGrouping {
        var layouts: [Int: TranscriptGrouping.RowLayout] = [:]
        var avatars: [Int: SenderAvatarModel] = [:]
        var bubbleGaps: [Int: CGFloat] = [:]
    }

    /// The raw feed as grouping items (the web mapping ×4): narration =
    /// the agent's message; a person's prompt = the viewer's own, unless
    /// it parses as a session input (`crossSessionInput`: that session's
    /// message); tool / thought / subagent / edits runs = `trace`;
    /// everything else (cards, pills, errors, a `Workflow` call's Work
    /// card) and the tool-posted cards = `full`. ONE origin rule (data first): the S-3 `origin` from the
    /// synced rows + `fromStart` (the window holds the feed's first row)
    /// go to the grouping, which reads the first prompt as the origin's.
    private func feedGrouping(
        _ entries: [TranscriptEntry], anchors: (anchors: [Int: [String]], unanchored: [String]), model: AgentSessionModel
    ) -> FeedGrouping {
        let viewerId = deps.auth.userId
        var items: [TranscriptGrouping.Item] = anchors.unanchored.map { TranscriptGrouping.Item(id: "card:\($0)", kind: .full) }
        var extraSessions: [TranscriptGrouping.KnownSession] = []
        for entry in entries {
            let id = "row:\(entry.id)"
            var item = TranscriptGrouping.Item(id: id, kind: .full)
            switch entry.row {
            case .toolRun, .edits, .expRun, .subagentRun:
                item = TranscriptGrouping.Item(id: id, kind: .trace)
            case .ask:
                break
            case let .single(feedItem):
                switch feedItem {
                case .narration:
                    item = TranscriptGrouping.Item(id: id, kind: .message, authorKind: DomainContract.messageAuthorKindAgent)
                case let .userMessage(_, text, _, _):
                    if Chats.commandDivider(text) != nil || SlashCommands.command(for: text, agent: model.catalogAgent) != nil {
                        break
                    }
                    if case .marker = TranscriptGrouping.feedInput(text) { break }
                    if let input = TranscriptGrouping.crossSessionInput(text) {
                        item = TranscriptGrouping.Item(
                            id: id, kind: .message, authorKind: DomainContract.messageAuthorKindAgent,
                            authorSessionId: input.sessionId
                        )
                        extraSessions.append(TranscriptGrouping.KnownSession(id: input.sessionId, title: input.name, agentId: nil))
                    } else {
                        item = TranscriptGrouping.Item(
                            id: id, kind: .message, authorKind: DomainContract.messageAuthorKindUser, authorId: viewerId
                        )
                    }
                case let .tool(_, _, _, _, callId, _, _, _, _, _, _, _, _):
                    // EXP-1750: a `Workflow` call draws as THE Work card,
                    // a full row (web, desktop, Android alike).
                    if model.workflow(for: callId) == nil {
                        item = TranscriptGrouping.Item(id: id, kind: .trace)
                    }
                case .thought, .subagent:
                    item = TranscriptGrouping.Item(id: id, kind: .trace)
                default:
                    break
                }
            }
            items.append(item)
            items += Self.rowItemIds(entry.row).flatMap { anchors.anchors[$0] ?? [] }
                .map { TranscriptGrouping.Item(id: "card:\($0)", kind: .full) }
        }
        let viewed = model.session ?? session
        let input = transcriptRows?.transcriptInput(
            items: items, viewerId: viewerId, chat: viewed, originSessionId: originSessionId,
            fromStart: model.transcriptFromStart, extraSessions: extraSessions
        ) ?? TranscriptGrouping.Input(
            viewerId: viewerId,
            chat: TranscriptGrouping.Chat(
                sessionId: viewed.id,
                title: viewed.title ?? SessionTitle.Copy.untitled,
                agentId: viewed.agentId,
                originSessionId: nil
            ),
            members: [], agents: [], sessions: extraSessions, items: items
        )
        var out = FeedGrouping()
        var previous: TranscriptGrouping.Item?
        for (item, layout) in zip(items, TranscriptGrouping.transcriptLayout(input)) {
            defer { previous = item }
            guard item.id.hasPrefix("row:"), let entryId = Int(item.id.dropFirst(4)) else { continue }
            out.layouts[entryId] = layout
            if layout.avatar, let sender = layout.sender {
                out.avatars[entryId] = transcriptRows?.senderAvatar(sender) ?? Self.fallbackAvatar(sender)
            }
            if item.kind == .message, previous?.kind == .message, layout.side != .full {
                out.bubbleGaps[entryId] = layout.gap
            }
        }
        return out
    }

    /// #EXP-1736: a subagent's own tab through the same grouping: the
    /// subagent is the chat's own sender (its prose = its bubbles, its
    /// calls = its trace), the turns addressed to it the viewer's own.
    static func laneGrouping(_ rows: [AgentFeedRow], run: AgentSubagentRun) -> FeedGrouping {
        let viewer = "viewer"
        let items: [TranscriptGrouping.Item] = rows.map { row in
            let id = "row:\(row.id)"
            switch row {
            case .toolRun, .edits, .expRun, .subagentRun:
                return TranscriptGrouping.Item(id: id, kind: .trace)
            case .ask:
                return TranscriptGrouping.Item(id: id, kind: .full)
            case let .single(item):
                switch item {
                case .narration:
                    return TranscriptGrouping.Item(id: id, kind: .message, authorKind: DomainContract.messageAuthorKindAgent)
                case .userMessage:
                    return TranscriptGrouping.Item(
                        id: id, kind: .message, authorKind: DomainContract.messageAuthorKindUser, authorId: viewer
                    )
                case .tool, .thought, .subagent:
                    return TranscriptGrouping.Item(id: id, kind: .trace)
                default:
                    return TranscriptGrouping.Item(id: id, kind: .full)
                }
            }
        }
        let title = (run.title ?? "").trimmingCharacters(in: .whitespaces)
        let name = title.isEmpty ? SubagentTraceLine.label(run.agentType) : title
        let input = TranscriptGrouping.Input(
            viewerId: viewer,
            chat: TranscriptGrouping.Chat(
                sessionId: "subagent:\(run.subagentId)", title: name, agentId: nil, originSessionId: nil
            ),
            members: [], agents: [], sessions: [], items: items
        )
        var out = FeedGrouping()
        var previous: TranscriptGrouping.Item?
        for (item, layout) in zip(items, TranscriptGrouping.transcriptLayout(input)) {
            defer { previous = item }
            guard let rowId = Int(item.id.dropFirst(4)) else { continue }
            out.layouts[rowId] = layout
            if layout.avatar { out.avatars[rowId] = .session }
            if item.kind == .message, previous?.kind == .message, layout.side != .full {
                out.bubbleGaps[rowId] = layout.gap
            }
        }
        return out
    }

    /// A sender's avatar before the tree snapshot loaded.
    private static func fallbackAvatar(_ sender: TranscriptGrouping.Sender) -> SenderAvatarModel {
        switch sender.kind {
        case .member: .member(user: nil, id: sender.id)
        case .agent: .agent(UserAvatarAgent(icon: nil, color: nil))
        case .session: .session
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
        guard model?.phase == .live else { return nil }
        return model?.openToolRowId
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

    /// #EXP-1625: a tool item's wire call id (what `testResults` keys on).
    private static func callId(_ item: AgentFeedItem) -> String? {
        guard case let .tool(_, _, _, _, callId, _, _, _, _, _, _, _, _) = item else { return nil }
        return callId
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

    /// waiting-cards.json DEEP LINKS `found`: the card draws in the main
    /// transcript now, at the head (`unanchored`) or after a rendered row.
    private func drawsCard(_ id: String, model: AgentSessionModel) -> Bool {
        guard agentTab == nil, toolAsks.contains(where: { $0.id == id }) || surfaceRows[id] != nil else {
            return false
        }
        let placed = AskDock.anchorSessionCards(model.feedCalls, cards: sessionCards)
        if placed.unanchored.contains(id) { return true }
        return model.transcript.contains { entry in
            Self.rowItemIds(entry.row).contains { placed.anchors[$0]?.contains(id) == true }
        }
    }

    /// EXP-1280: the session cards with these message ids, inline as THE
    /// chat card (`ChatAskCard`): a waiting one answerable by a viewer
    /// who may post, an answered one
    /// closed with its picks; a `surface` card = THE chat's
    /// `SurfaceCardRow`.
    @ViewBuilder
    private func inlineToolCards(_ ids: [String], model: AgentSessionModel) -> some View {
        let canAnswerCards = model.mayPost && !model.sessionEnded
        ForEach(ids, id: \.self) { id in
            if case let .surface(_, _, title, surface)? = surfaceRows[id] {
                SurfaceCardRow(
                    surface: surface,
                    title: title,
                    animate: false,
                    onReply: canAnswerCards ? { text in replyFromSurface(text) } : nil
                )
                .waitingHighlight(highlightId == id)
                .deepLinkRow(deepLink?.target == id, frame: $deepLinkFrame)
                .id(id)
                .padding(.top, DesignTokens.Transcript.gapBlock)
                .accessibilityIdentifier("session-surface-card")
            } else if let row = toolAsks.first(where: { $0.id == id }) {
                ChatAskCard(
                    row: row,
                    canAnswer: canAnswerCards,
                    answeringId: toolAnsweringId,
                    markdownContext: markdownContext,
                    onSubmit: submitToolAnswer
                )
                .waitingHighlight(highlightId == id)
                .deepLinkRow(deepLink?.target == id, frame: $deepLinkFrame)
                .id(id)
                .padding(.top, DesignTokens.Transcript.gapBlock)
                .accessibilityIdentifier("session-tool-card")
            }
        }
    }

    /// #EXP-1380 (TS `loadEarlierMessages`): the transcript's Load earlier
    /// also widens the chat's loaded messages by one `windowStep` while
    /// older ones exist unloaded.
    private func loadEarlierMessages() {
        guard let rows = transcriptRows, rows.messagesMore else { return }
        Chats.MessageLimits.shared.loadEarlier(rows.root.id)
    }

    /// #EXP-1736: this session's tree snapshot (the transcript's senders)
    /// and S-3 `origin` (`buildSessionChat` focused on it), re-read only
    /// when its root's digest moves (`ChatRows.signature`).
    private func observeTranscriptSenders() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let rootId = session.rootId.isEmpty ? session.id : session.rootId
        let id = session.id
        let observation = ValueObservation
            .tracking { db in try ChatRows.signature(db: db, rootId: rootId) }
            .removeDuplicates()
        // #PERF-9: inside a chat already on screen, its snapshot draws the
        // senders from the first frame; the digest's own read follows.
        if transcriptRows == nil, let rows = await ChatRowsLoads.shared.latest(
            pool: pool, rootId: rootId, limit: Chats.MessageLimits.shared.limit(rootId)
        ) {
            transcriptRows = rows
        }
        do {
            for try await signature in observation.values(in: pool) {
                // Large chats: the chat screen hosting this view observes the
                // same digest; one read serves both.
                guard let rows = try await ChatRowsLoads.shared.load(
                    pool: pool, rootId: rootId, signature: signature,
                    limit: Chats.MessageLimits.shared.limit(rootId)
                ) else {
                    transcriptRows = nil
                    originSessionId = nil
                    continue
                }
                let (link, suggestions) = await Task.detached(priority: .utility) {
                    let view = rows.view(now: Date(), focus: id == rows.root.id ? nil : id)
                    // EXP-1323: only a ROOT's own Conversation holds them.
                    let suggestions: [SessionChatRow] = id == rows.root.id
                        ? view.rows.filter { row in
                            if case .suggestions = row { return true }
                            return false
                        }
                        : []
                    return (view.origin, suggestions)
                }.value
                transcriptRows = rows
                originSessionId = link?.sessionId
                suggestionRows = suggestions
            }
        } catch {}
    }

    /// #VAPP-229: the name of the live app `viaAppId` names (a deleted or
    /// unknown app = nil, no caption).
    private func observeViaApp() async {
        guard let appId = session.viaAppId, let pool = try? deps.db.pool(forAccountId: accountId) else {
            viaAppName = nil
            return
        }
        let observation = ValueObservation.tracking { db in
            try AppEntity.filter(Column("id") == appId && Column("deleted_at") == nil).fetchOne(db)?.name
        }
        do {
            for try await name in observation.values(in: pool) {
                viaAppName = name
            }
        } catch {}
    }

    /// EXP-1280: observe this session's own synced card messages; keep its
    /// cards (EXP-1398 `AskDock.sessionCards`) as rows.
    private func observeToolAsks() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let id = session.id
        // Large chats: only the session's card rows, opened off main (a
        // chat's whole conversation, re-opened per write, held the main
        // thread ~120 ms on an 8.5k-message chat).
        let observation = ValueObservation.tracking { db in
            try SessionQueries.cardRows(db: db, sessionId: id).map(TreeMessage.init(messageRow:))
        }
        do {
            for try await tree in observation.values(in: pool) {
                let byId = Dictionary(tree.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
                let cards = AskDock.sessionCards(tree)
                var asks: [SessionChatRow] = []
                var surfaces: [String: SessionChatRow] = [:]
                for card in cards {
                    guard let message = byId[card.id] else { continue }
                    if card.kind == .surface {
                        surfaces[card.id] = SessionChatRow.surfaceCard(message)
                    } else if let row = SessionChatRow.askCard(message) {
                        asks.append(row)
                    }
                }
                sessionCards = cards.filter { $0.kind != .surface || surfaces[$0.id] != nil }
                toolAsks = asks
                surfaceRows = surfaces
                if !cardsLoaded { cardsLoaded = true }
            }
        } catch {}
    }

    /// EXP-1398: a surface card's reply = the person's next message, through
    /// the one send path (ChatScreen `reply`).
    private func replyFromSurface(_ text: String) {
        let api = deps.sessionsApi
        let accountId = accountId
        let sessionId = session.id
        Task {
            do {
                _ = try await api.send(accountId: accountId, .message(sessionId: sessionId, text: text))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the message"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    /// EXP-1280: answer a tool-posted card (`sessions.answer`); a refusal
    /// toasts and the card stays answerable.
    private func submitToolAnswer(_ card: ChatCardAnswer) {
        guard toolAnsweringId == nil else { return }
        toolAnsweringId = card.messageId
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                if card.dismissed {
                    try await api.dismissCard(accountId: accountId, messageId: card.messageId)
                } else {
                    try await api.answer(
                        accountId: accountId, messageId: card.messageId, answer: card.answer, decision: card.decision,
                        fresh: card.fresh, feedback: card.feedback, answers: card.answers, surfaceData: card.surfaceData
                    )
                }
            } catch {
                toaster.error(
                    card.dismissed
                        ? FeedbackStates.Errors.title(verb: "dismiss", thing: "the card")
                        : FeedbackStates.Errors.title(
                            verb: "answer", thing: card.decision == nil ? "the question" : "the plan"
                        ),
                    description: error.userFacingMessage
                )
            }
            toolAnsweringId = nil
        }
    }

    /// #FEED-237 (message-delivery.json): the viewer's own pending bubbles,
    /// the `HH:mm` of the row (an outbox entry's own until its row arrives)
    /// then the mark; the menu per `bubbleActions`.
    @ViewBuilder
    private func ownPendingBubbles(_ model: AgentSessionModel) -> some View {
        ForEach(Array(model.ownBubbles.enumerated()), id: \.element.id) { index, bubble in
            TranscriptRow(
                side: .own,
                gap: index == 0 ? TranscriptGrouping.Layout.gapBetween : TranscriptGrouping.Layout.gapInGroup,
                avatar: nil
            ) {
                UserMessageBubble(
                    text: bubble.text,
                    context: markdownContext,
                    caption: bubble.at.map { GlassTimeRow.formatted($0) },
                    status: bubble.state,
                    actions: bubble.actions,
                    onAction: { action in ownBubbleAction(action, bubble: bubble, model: model) }
                )
            }
            .id("own-\(bubble.id)")
        }
    }

    private func ownBubbleAction(_ action: MessageDelivery.Action, bubble: OwnPendingBubble, model: AgentSessionModel) {
        switch action {
        case .retry:
            if let clientId = bubble.clientId { model.retryOutbox(clientId) }
        case .delete:
            if let clientId = bubble.clientId { model.deleteOutbox(clientId) }
        case .cancel:
            Task {
                if await !model.cancelSending(bubble) {
                    toaster.error(MessageDelivery.Copy.cancelFailed)
                }
            }
        }
    }

    /// N-12: THE Dock's rows: THE Queue and the banners (`SessionDock`). S-3 (dock.json): a waiting plan or question
    /// is never a DockCard; it stays inline in the transcript.
    private func dockRows(_ model: AgentSessionModel) -> [Dock.Row] {
        // EXP-1323 / #EXP-1732: a root's suggested tasks with an open item
        // lead (the old Conversation's order), for a viewer who may act.
        let suggestions = canSuggest(model) ? Dock.suggestionSources(suggestionRows) : []
        return Dock.rows(suggestions + SessionDock.sources(model, session: session))
    }

    /// A DockCard's sheet: THE Queue / the queued messages their lists.
    private func dockSheet(_ row: Dock.Row, model: AgentSessionModel, close: @escaping () -> Void) -> AnyView? {
        suggestionsSheet(row, close: close) ?? SessionDock.sheet(row, model: model, close: close)
    }

    /// A banner's one button: Reconnect, the usage wall's account switch.
    private func dockAction(_ row: Dock.Row, model: AgentSessionModel) -> AnyView? {
        switch row.id {
        case "banner:closed": SessionDock.reconnect(model)
        case "banner:blocked" where row.action != nil: AnyView(accountSwitchControl(model))
        case "banner:offline" where row.action != nil: deviceMoveControl(model, label: row.action ?? "")
        default: nil
        }
    }

    /// A lone question card: a plan approval or a single-question ask.
    @ViewBuilder
    private func questionCard(_ question: AgentQuestion, chromeless: Bool = false) -> some View {
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
            chromeless: chromeless
        )
        .id(question.id)
        // EXP-642: the store slide's pop-out rect is measured off the question
        // card (`PopRects`). `contain` keeps its option buttons queryable.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("feed-question")
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
    private func askCard(_ group: AgentAskGroup, chromeless: Bool = false) -> some View {
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
                    markdownContext: markdownContext,
                    chromeless: chromeless
                )
                // Its own identity per edited step — the picked state starts
                // from the recorded answer, never from the current step's.
                .id("edit-\(editingStep.id)")
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("feed-question")
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
                    chromeless: chromeless
                )
                // A fresh identity per step — the card's local selection state must
                // never leak from one question into the next.
                .id(current.id)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("feed-question")
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

    // MARK: - Account switch (EXP-849)

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

    // MARK: - Device move (#EXP-1703)

    /// #EXP-1703: the offline banner's "Move" (session-card.json
    /// `bannerActions`): THE device picker behind a pill, the rows and
    /// refusals of `SessionDeviceMove` (the composer's Device pin), a pick =
    /// `sessions.send({sessionId, deviceId})`; a failure is the move toast
    /// (`moveError`). Hidden when the viewer may not move it (web
    /// `SessionDeviceMoveButton`).
    private func deviceMoveControl(_ model: AgentSessionModel, label: String) -> AnyView? {
        guard let pin = model.devicePin, pin.blockedReason == nil else { return nil }
        return AnyView(
            GlassPill(
                label,
                icon: AppIcons.uiSwap,
                mode: .action { showDeviceMove = true },
                enabled: !pin.sending
            )
            .accessibilityIdentifier("session-device-move")
            .background(
                DevicePicker(
                    devices: pin.rows,
                    value: pin.currentDeviceId,
                    onChange: pin.onPick,
                    title: SessionDeviceMove.pickerTitle,
                    open: $showDeviceMove,
                    hideTrigger: true,
                    trigger: { EmptyView() }
                )
            )
        )
    }

    // MARK: - Bottom bar (steering input)

    /// N-12: the composer never yields to a waiting card (session-chat.json
    /// retires `composerYieldsToCard`). EXP-621: it is tied to the SESSION,
    /// not the socket — it stays up through a reconnect and only goes away
    /// once the session is over. EXP-1295: a viewer who may not post (a
    /// private session) never gets one.
    private func bandRetired(_ model: AgentSessionModel) -> Bool {
        model.isOver || !model.mayPost
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
                // chat's Conversation.
                FloatingComposer(
                    placeholder: model.composerPlaceholder,
                    open: composerOpen(model),
                    collapsedLabel: "Message the agent",
                    collapsedIdentifier: "agent-composer-collapsed",
                    onExpand: expandComposer
                ) {
                    composerCard(model)
                }
                // #FEED-219: the session's synced draft.
                .composerDraft(
                    .session(session.id),
                    teamId: session.teamId,
                    revision: model.draftText,
                    read: { model.draftText },
                    apply: { model.draftText = $0 }
                )
            }
            .animation(motion.standard, value: composerExpanded)
        }
    }

    @ViewBuilder
    private func composerMenuView(_ model: AgentSessionModel) -> some View {
        switch composerMenu(model) {
        case .slash:
            SlashCommandMenu(
                candidates: slashCandidates(model),
                highlighted: slashHighlight
            ) { candidate in
                applySlashCandidate(candidate, model)
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

    /// The menu is up while the draft matches something and it hasn't been
    /// dismissed at exactly this draft; the keyboard going away keeps it
    /// (EXP-1676: web's `useSlashCommandMenu`, Android).
    private func slashMenuVisible(_ model: AgentSessionModel) -> Bool {
        !slashCandidates(model).isEmpty
            && slashDismissedFor != model.draftText
    }

    /// NEW-2: THE `/` menu's rows — the builtins this agent runs, then the
    /// team's prompts (`SlashCommands.candidates`).
    private func slashCandidates(_ model: AgentSessionModel) -> [SlashCandidate] {
        SlashCommands.candidates(draft: model.draftText, agent: model.catalogAgent, prompts: slashPrompts.prompts)
    }

    /// A command rewrites the draft; a prompt's body (`prompts.get`) REPLACES
    /// the `/name` draft the menu was open on, to be edited and sent (web
    /// `session-composer.tsx`). Never sends.
    private func applySlashCandidate(_ candidate: SlashCandidate, _ model: AgentSessionModel) {
        switch candidate {
        case let .command(command):
            applySlashCommand(command, model)
        case let .prompt(prompt):
            slashDismissedFor = model.draftText
            slashHighlight = 0
            Task {
                guard let body = await TeamSlashPrompts.loadBody(prompt, accountId: accountId, deps: deps),
                      SlashCommands.isSlashDraft(model.draftText)
                else { return }
                model.draftText = body
                focusComposer()
            }
        }
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
    private func highlightedSlashCandidate(_ model: AgentSessionModel) -> SlashCandidate? {
        let matches = slashCandidates(model)
        guard matches.indices.contains(slashHighlight) else { return matches.first }
        return matches[slashHighlight]
    }

    /// ↑/↓ wrap around the list (web/Android/desktop parity).
    private func moveSlashHighlight(_ delta: Int, _ model: AgentSessionModel) {
        let count = slashCandidates(model).count
        guard count > 0 else { return }
        slashHighlight = ((slashHighlight + delta) % count + count) % count
    }

    // MARK: - Composer card

    /// EXP-1295: THE session composer box (`SessionComposerCard`) with the
    /// session's own controls: the "+" Model row (S-3), the usage ring
    /// (EXP-893), the `/` keys (EXP-724) and Stop in the send slot.
    private func composerCard(_ model: AgentSessionModel) -> some View {
        // EXP-790: an EMPTY field while the agent works offers Stop (the
        // interrupt) in the send slot; the first typed character brings Send
        // back.
        let showsStop = model.agentWorking && !model.hasContent
        // #EXP-1729: an ended editor root's Account and Model = the launch
        // picks riding its next send.
        let launchPicks = model.session?.isLive == false ? editorPicks : nil
        return SessionComposerCard(
            draft: model,
            // EXP-820: the generic prompt — a pending card's free answer is
            // the card's own inline field.
            placeholder: model.composerPlaceholder,
            controls: SessionComposerFooter.Controls(
                modelValue: launchPicks?.model ?? model.composerModelValue(),
                agent: launchPicks?.agent ?? model.catalogAgent,
                onPickModel: { alias in
                    if let launchPicks {
                        launchPicks.onPickModel(alias)
                        return
                    }
                    sendModelSwitch(model, alias)
                },
                usageFraction: usageFraction(model),
                usageSeverity: usageSeverity(model),
                onUsage: { showUsageSheet = true },
                // An ended row takes no `/model` (no Model row in the "+").
                ended: model.session?.isLive == false,
                // S-3: an agent's conversation root is an agent chat's "+".
                surface: session.parentId == nil && !(session.agentId ?? "").isEmpty ? .agent : .session,
                // S-3: the "+" Issue page searches the session's team.
                teamId: model.session?.teamId,
                // #EXP-1434: the "+" Prompts page = the `/` menu's prompts; a
                // pick inserts the body at the caret.
                prompts: slashPrompts.prompts,
                onPickPrompt: { prompt in
                    Task {
                        guard let body = await TeamSlashPrompts.loadBody(prompt, accountId: accountId, deps: deps)
                        else { return }
                        model.draftEditor.insertTextAtCaret(body)
                        focusComposer()
                    }
                },
                // #EXP-1535: the "+" Account page = the usage wall's switch.
                account: launchPicks?.account ?? model.composerAccountMenu ?? editorPicks?.liveAccount,
                // #EXP-1715: the viewer's Assistant root's Teams pick.
                teams: teamsPick,
                device: editorDevice.map { menu in
                    SessionComposerFooter.DeviceMenu(devices: menu.devices, current: menu.current) { picked in
                        menu.onPick(picked)
                        guard model.session?.isLive == true else { return }
                        Task { await model.moveDevice(to: picked) }
                    }
                },
                mcp: mcpMenu(model),
                editor: editorPicks != nil
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
                openIssueInnermost(issueId, outlet: chatIssueOutlet, deps: deps, accountId: accountId)
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
            },
            // #EXP-1625: the field beside `agent-composer-collapsed` (the
            // chat's pair is `chat-composer-*`).
            fieldIdentifier: "agent-composer-field"
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
            guard let candidate = highlightedSlashCandidate(model) else { return }
            applySlashCandidate(candidate, model)
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
        prepareSend(model)
        Task { await model.sendDraft() }
    }

    /// #EXP-1715 / #FEED-204: what the next send carries: the Assistant
    /// root's Teams pick, an app editor's move to another device.
    private func prepareSend(_ model: AgentSessionModel) {
        model.sendScope = teamsPick?.scope
        model.sendAppMove = editorMove
    }

    /// #EXP-1793 (composer-menu.json `mcpPick`): the agent whose conversation
    /// ROOT this is, while it is the viewer's own; nil anywhere else (a
    /// child, another member's conversation, any other session).
    private var mcpPickAgentId: String? {
        guard session.parentId == nil, session.userId == deps.auth.userId,
              let agentId = session.agentId, !agentId.isEmpty
        else { return nil }
        return agentId
    }

    /// The "+" MCP servers page of the viewer's agent conversation root.
    private func mcpMenu(_ model: AgentSessionModel) -> SessionComposerFooter.McpMenu? {
        guard mcpPickAgentId != nil, let pick = model.mcpPick else { return nil }
        return SessionComposerFooter.McpMenu(
            servers: mcpServers,
            picked: pick.picked,
            onChange: { model.mcpPick?.picked = $0 }
        )
    }

    /// The team's servers and the conversation's pick (`sessions.mcpPicks`;
    /// unreadable = the agent's `mcpServerIds`) seed the "+" pick.
    private func loadMcpPick() async {
        guard let agentId = mcpPickAgentId, let model else { return }
        let api = McpServersApi(trpc: deps.trpc)
        async let servers = try? api.list(accountId: accountId, teamId: session.teamId)
        async let picks = try? deps.sessionsApi.mcpPicks(accountId: accountId, sessionId: session.id)
        let (rows, picked) = await (servers, picks)
        mcpServers = rows ?? []
        let seed = picked ?? transcriptRows?.agent?.mcpServerIds ?? []
        if model.mcpPick?.agentId == agentId {
            model.mcpPick?.reseed(seed)
        } else {
            model.mcpPick = AgentChatMcpPick(agentId: agentId, seed: seed)
        }
    }

    /// #EXP-1715 (composer-menu.json `teamsPick`): the Teams pick while this
    /// view is the viewer's OWN Assistant conversation ROOT; nil anywhere
    /// else (a child included: the server refuses a scope there).
    private var teamsPick: ComposerTeamsBinding? {
        guard session.parentId == nil,
              AgentSettings.isMyAssistant(transcriptRows?.agent, me: deps.auth.userId)
        else { return nil }
        return ComposerTeamsBinding(
            teams: ComposerTeamsBinding.teams(teamState?.teams ?? []),
            picked: teamsPicked,
            onChange: { teamsPicked = $0 }
        )
    }

    // MARK: - Suggested tasks (EXP-1323, the root's)

    /// A viewer who may post acts on the suggestions (and they dock).
    private func canSuggest(_ model: AgentSessionModel) -> Bool { model.mayPost }

    private var suggestionActions: ChatSuggestionActions {
        ChatSuggestionActions(
            onStart: { rowId, itemId in startSuggestion(rowId, itemId, where: .child) },
            onStartAll: { rowId in
                runSuggestion(ChatSuggestionActions.key(rowId), verb: "start", thing: "the tasks") { api, accountId in
                    try await api.startAllSuggestions(accountId: accountId, messageId: rowId)
                }
            },
            onMenu: { rowId, itemId, action in runSuggestionMenu(rowId, itemId, action) },
            devices: { [teamId = session.teamId] in await loadSuggestionDevices(teamId: teamId) },
            pending: suggestionPending
        )
    }

    /// The suggestions rows that draw INLINE: every one for a reader who
    /// cannot act, else the ones with no open item left (the rest dock).
    private func inlineSuggestions(_ model: AgentSessionModel) -> [SessionChatRow] {
        guard canSuggest(model) else { return suggestionRows }
        return suggestionRows.filter { Dock.suggestionSources([$0]).isEmpty }
    }

    @ViewBuilder
    private func inlineSuggestionCards(_ model: AgentSessionModel) -> some View {
        ForEach(inlineSuggestions(model), id: \.id) { row in
            if case let .suggestions(id, _, header, startAll, items) = row {
                ChatSuggestionsCard(
                    rowId: id,
                    header: header,
                    startAll: startAll,
                    items: items,
                    actions: canSuggest(model) ? suggestionActions : nil,
                    onOpenSession: { sessionId in openSuggestionSession(sessionId) },
                    onOpenIssue: { issueId in
                        openIssueInnermost(issueId, outlet: chatIssueOutlet, deps: deps, accountId: accountId)
                    }
                )
                .padding(.top, DesignTokens.Transcript.gapBlock)
            }
        }
    }

    /// The suggestions DockCard's sheet (N-12).
    private func suggestionsSheet(_ row: Dock.Row, close: @escaping () -> Void) -> AnyView? {
        guard case let .suggestions(id, _, _, startAll, items)? = suggestionRows.first(where: { $0.id == row.id })
        else { return nil }
        return AnyView(ChatSuggestionsSheet(
            row: row,
            rowId: id,
            startAll: startAll,
            items: items,
            actions: suggestionActions,
            onOpenSession: { sessionId in
                close()
                openSuggestionSession(sessionId)
            },
            onOpenIssue: { [deps, accountId, chatIssueOutlet] issueId in
                close()
                openIssueInnermost(issueId, outlet: chatIssueOutlet, deps: deps, accountId: accountId)
            },
            onClose: close
        ))
    }

    /// A started item's session (inbox-route.json `sessionTarget`).
    private func openSuggestionSession(_ sessionId: String) {
        Task { pushRoute(await AppRoute.openingSession(sessionId, accountId: accountId, deps: deps)) }
    }

    private func startSuggestion(
        _ rowId: String, _ itemId: String, where target: SuggestionStartWhere,
        deviceId: String? = nil, model: String? = nil
    ) {
        runSuggestion(ChatSuggestionActions.key(rowId, itemId), verb: "start", thing: "the task") { api, accountId in
            try await api.startSuggestion(
                accountId: accountId, messageId: rowId, itemId: itemId, where: target,
                deviceId: deviceId, model: model
            )
        }
    }

    private func dismissSuggestion(_ rowId: String, _ itemId: String) {
        runSuggestion(ChatSuggestionActions.key(rowId, itemId), verb: "dismiss", thing: "the task") { api, accountId in
            try await api.dismissSuggestion(accountId: accountId, messageId: rowId, itemId: itemId)
        }
    }

    /// One suggestion call: its key waits while it is out (the Electric echo
    /// moves the item), a refusal toasts `Couldn't <verb> <thing>`.
    private func runSuggestion(
        _ key: String, verb: String, thing: String,
        _ call: @escaping @Sendable (SessionsApi, String) async throws -> Void
    ) {
        guard !suggestionPending.contains(key) else { return }
        suggestionPending.insert(key)
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await call(api, accountId)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: verb, thing: thing),
                    description: error.userFacingMessage
                )
            }
            suggestionPending.remove(key)
        }
    }

    /// A pick of the item's ⋯ menu (run once its sheet has dismissed).
    private func runSuggestionMenu(_ rowId: String, _ itemId: String, _ action: SuggestionOptionsSheet.Action) {
        switch action {
        case .here:
            startSuggestion(rowId, itemId, where: .here)
        case let .startOn(deviceId):
            startSuggestion(rowId, itemId, where: .child, deviceId: deviceId)
        case let .startWith(model):
            startSuggestion(rowId, itemId, where: .child, model: model)
        case .file:
            runSuggestion(ChatSuggestionActions.key(rowId, itemId), verb: "file", thing: "the issue") { api, accountId in
                try await api.fileSuggestion(accountId: accountId, messageId: rowId, itemId: itemId)
            }
        case .dismiss:
            dismissSuggestion(rowId, itemId)
        }
    }

    /// Start on…'s rows: the machines a start can reach right now with an
    /// agent to run (the composer's pool).
    private func loadSuggestionDevices(teamId: String?) async -> [SuggestionOptionsSheet.Device] {
        let devices = await DeviceQueries.onlineStartTargets(
            db: deps.db, accountId: accountId, teamId: teamId, userId: deps.auth.userId
        )
        return devices.filter(\.hasRunnableAgent).map {
            SuggestionOptionsSheet.Device(
                id: $0.deviceId, name: $0.deviceLabel, icon: DeviceIconDisplay.iconName(for: $0)
            )
        }
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
/// never precomputed. (#VAPP-258: the read-only Markdown takes the theme's
/// `Markdown/*` chat rhythm instead.)
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
/// EXP-783 / #EXP-1380: THE transcript's `Load earlier` ghost button — the
/// run view's, and every chat's window (chats.json WINDOW ×4).
struct TranscriptLoadEarlierButton: View {
    let action: () -> Void

    var body: some View {
        Button(Chats.Copy.loadEarlier, action: action)
            .font(.footnote)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity)
            .padding(.bottom, 4)
            .accessibilityIdentifier(Chats.TestIds.loadEarlier)
    }
}

struct NarrationBubble: View {
    let text: String
    let context: AgentMarkdownContext
    /// #VAPP-258: the reply still being written (`streaming: true`).
    var streaming = false

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
                streaming: streaming
            )
        }
    }
}

/// A human turn (EXP-78): the initial prompt or a steered message — the
/// viewer's own side of THE message bubble (#EXP-1736 `MessageBubble`
/// `.own`: right, at most 78% of the column), long text folded.
struct UserMessageBubble: View {
    let text: String
    let context: AgentMarkdownContext
    /// EXP-1553 (chats.json TRANSCRIPT ×4): the bubble's caption (`HH:mm`)
    /// INSIDE it bottom-right; nil = none.
    var caption: String?
    /// #FEED-237 (message-delivery.json): the delivery mark after the time
    /// (the viewer's own bubbles); nil = none.
    var status: MessageDelivery.State? = nil
    /// The bubble's menu (`bubbleActions`; long-press, a failed mark's
    /// press); empty = none.
    var actions: [MessageDelivery.Action] = []
    var onAction: (MessageDelivery.Action) -> Void = { _ in }

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
        let hasCaption = (caption?.isEmpty == false) || status != nil
        return MessageBubble(side: .own, hasFooter: hasCaption) {
            VStack(alignment: .leading, spacing: 4) {
                Group {
                    if !hasImages {
                        AgentMarkdownText(
                            text: text,
                            context: context,
                            hugsWidth: true
                        )
                    } else if !parsed.text.isEmpty {
                        if parsed.markers.isEmpty {
                            AgentMarkdownText(
                                text: parsed.text,
                                context: context,
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
                        hugsWidth: true
                    )
                    .padding(.top, parsed.text.isEmpty ? 0 : 4)
                }
                // Wave D: each file line = the comment's file row
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
        } footer: {
            BubbleCaptionLine(caption: caption, status: status, actions: actions, onAction: onAction)
        }
        .modifier(MessageBubbleContextMenu(actions: actions, onAction: onAction))
    }
}

/// The long-press menu, only on a bubble that has one.
private struct MessageBubbleContextMenu: ViewModifier {
    let actions: [MessageDelivery.Action]
    let onAction: (MessageDelivery.Action) -> Void

    func body(content: Content) -> some View {
        if actions.isEmpty {
            content
        } else {
            content.contextMenu {
                MessageBubbleMenu(actions: actions, onAction: onAction)
            }
        }
    }
}

/// #FEED-237: the own bubble's menu (message-delivery.json `copy.menu`).
private struct MessageBubbleMenu: View {
    let actions: [MessageDelivery.Action]
    let onAction: (MessageDelivery.Action) -> Void

    var body: some View {
        ForEach(actions, id: \.self) { action in
            Button(role: action == .delete ? .destructive : nil) {
                onAction(action)
            } label: {
                Text(MessageDelivery.Copy.menu(action))
            }
            .accessibilityIdentifier("\(MessageDelivery.TestIds.menu)-\(action.rawValue)")
        }
    }
}

/// The caption line itself: the muted time, then the delivery mark (a
/// failed one is a press target opening the bubble's menu).
private struct BubbleCaptionLine: View {
    let caption: String?
    var status: MessageDelivery.State?
    var actions: [MessageDelivery.Action] = []
    var onAction: (MessageDelivery.Action) -> Void = { _ in }

    var body: some View {
        HStack(spacing: 4) {
            if let caption, !caption.isEmpty {
                Text(caption)
                    .font(.caption2)
                    .monospacedDigit()
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityIdentifier("user-message-caption")
            }
            if let status {
                if status == .failed, !actions.isEmpty {
                    Menu {
                        MessageBubbleMenu(actions: actions, onAction: onAction)
                    } label: {
                        MessageStatusMark(status)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(MessageDelivery.TestIds.menu)
                } else {
                    MessageStatusMark(status)
                }
            }
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
    /// muted truncating caption beside the label (a chat's card).
    var caption: String? = nil
    /// N-12: inside THE Dock's sheet (its header is the card's chrome): no
    /// glyph column, no card, the body never folds (the sheet scrolls).
    var chromeless: Bool = false
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
    /// EXP-1280: the form shows the answer itself (a chat question card's
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
    /// #EXP-1464 (chat-row-action.json): trailing the label line (a
    /// conversation row's expanded card: `Open chat`).
    var headerTrailing: AnyView? = nil

    @State private var expanded = false
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

    /// Plans are always fully rendered — never folded (EXP-197); a sheet's
    /// body scrolls instead.
    private var clampable: Bool {
        !chromeless && !question.planMode
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
        if chromeless {
            cardContent
                .onAppear(perform: preselectEditingAnswer)
        } else {
            card
        }
    }

    private var card: some View {
        // EXP-1191 (web `AskCard` parity): ONE top-aligned row — the 14pt
        // glyph beside a column holding the heading, the body and the
        // options, so the options indent under the text, never the glyph.
        HStack(alignment: .top, spacing: 8) {
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
                        if let headerTrailing {
                            Spacer(minLength: 8)
                            headerTrailing
                        }
                    }
                    .padding(.bottom, 4)
                }
                cardContent
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            // Web `AskCard`: no heading = the × sits beside the prompt, on the
            // glyph's line, never on a line of its own.
            if !hasHeading, let headerTrailing {
                headerTrailing
            }
        }
        .padding(12)
        // A bordered card, not a group container: the ask is one free-content
        // block that has to stand off the transcript behind it.
        .glassCard()
        .onAppear(perform: preselectEditingAnswer)
    }

    private var hasHeading: Bool {
        headerText != nil || stepLabel != nil || caption?.isEmpty == false
    }

    /// The label line: its words, with the trailing control beside them.
    private var hasHeader: Bool { hasHeading }

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

    /// The body + (EXP-1280) the rich card's form.
    @ViewBuilder
    private var prompt: some View {
        if form != nil {
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
            style: richBody ? .prompt.imageCap(nil).planCard : .prompt
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
                ExpCheckbox(checked: checked)
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
            .font(.expMono(size: 10))
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
    /// #EXP-1736: the subagent's name on its group's first bubble (its own
    /// tab only); nil = none.
    var author: String? = nil

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
                item: item, context: context, nested: nested, live: live, testResults: testResults,
                author: author
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
    /// #EXP-1736: the bubble's name line in the subagent's own tab.
    var author: String? = nil

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
            if nested {
                NarrationBubble(text: text, context: context)
            } else {
                // #EXP-1736: the subagent's own tab = THE message bubble.
                MessageBubble(side: .other, author: author, time: nil) {
                    AgentMarkdownText(text: text, context: context, streaming: false, hugsWidth: true)
                }
            }
        case let .apiError(_, message, errorType, _):
            ApiErrorRow(message: message, errorType: errorType)
        case let .userMessage(_, text, _, _):
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
/// byte-identical on all four clients. #EXP-1772: THE inline marker ×4, a
/// system input's too (`label` = transcript-grouping.json `copy.markers`).
struct TranscriptMarkerRow: View {
    var label: String = AgentFeed.compactedLabel

    var body: some View {
        // The web row: the label between two hairlines (`bg-border/60`).
        HStack(spacing: 8) {
            hairline
            Text(label)
                .transcriptToolText()
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .layoutPriority(1)
            hairline
        }
        .accessibilityElement(children: .combine)
    }

    private var hairline: some View {
        Rectangle()
            .fill(DesignTokens.Palette.border.opacity(0.6))
            .frame(height: 1)
            .frame(maxWidth: .infinity)
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
                    .font(.expMono(.caption))
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
/// EXP-1316 (1n): the chat Conversation follows with the same tracker.
/// steer-enet.json REPLAY TAIL — the reader scrolling within ONE viewport
/// of the transcript's top pages older rows in: the same action as the Load
/// earlier button above the rows (it stays, for a transcript too short to
/// scroll and for iOS 17). Fires on ENTERING the band, once per entry, and
/// off the scroll's layout pass (the EXP-943 re-entrancy rule).
struct LoadEarlierNearTop: ViewModifier {
    /// Main transcript, scrolled away from the bottom, something to load.
    let enabled: Bool
    let load: () -> Void
    @State private var nearTop = false

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content
                .onScrollGeometryChange(for: Bool.self) { geo in
                    geo.contentOffset.y + geo.contentInsets.top < geo.containerSize.height
                } action: { _, near in
                    nearTop = near
                    guard near, enabled else { return }
                    Task { @MainActor in load() }
                }
                // Still in the band when the gate opens (a page landed and
                // there is more): one more step, never a loop of them.
                .onChange(of: enabled) { _, on in
                    guard on, nearTop else { return }
                    Task { @MainActor in load() }
                }
        } else {
            content
        }
    }
}

struct FollowPinTracker: ViewModifier {
    @Binding var atBottom: Bool
    /// Within this many points of the bottom still counts as pinned.
    let slack: CGFloat
    /// waiting-cards.json DEEP LINKS: a pending link holds the pin off — no
    /// re-arm, no repin, whatever arrives or measures under it.
    var hold = false
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
            guard stranded || atBottom, !userScrolling, !hold else { return }
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
                    if newPhase == .idle, atBottom, !lastPinned, !hold {
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
                        if !atBottom, !hold { atBottom = true }
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
        model.steerEnet = installNode?.steer
        model.steerTunnel = steerTunnelApi
        model.connStats = connStats
        return model
    }
}


/// #EXP-1625: what one transcript row renders from. `volatile` rows never
/// compare equal (they read state the key does not carry).
struct TranscriptRowKey: Equatable {
    let entry: TranscriptEntry
    /// #EXP-1736: the row's sender + name line.
    let layout: TranscriptGrouping.RowLayout?
    let liveTail: Bool
    let liveTool: Int?
    let working: Bool
    let testResults: [TestResults?]
    let agent: String
    let cli: String?
    let volatile: Bool

    static func == (a: Self, b: Self) -> Bool {
        !a.volatile && !b.volatile
            && a.liveTail == b.liveTail && a.liveTool == b.liveTool && a.working == b.working
            && a.agent == b.agent && a.cli == b.cli
            && a.testResults == b.testResults && a.entry == b.entry && a.layout == b.layout
    }
}

/// #EXP-1625: one transcript row behind an equality gate (`.equatable()`):
/// a pass of the list whose key for this row is unchanged skips the row's
/// body, whatever re-ran the list (the closures it builds compare unequal
/// by construction, so without the gate every visible row re-evaluated).
struct TranscriptRowSlot<Content: View>: View, Equatable {
    let key: TranscriptRowKey
    @ViewBuilder let content: () -> Content

    nonisolated static func == (a: Self, b: Self) -> Bool { a.key == b.key }

    var body: some View {
        content()
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

/// iOS 18+: `defaultScrollAnchor(.bottom, for: .initialOffset)`.
private struct InitialBottomOffset: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content.defaultScrollAnchor(.bottom, for: .initialOffset)
        } else {
            content
        }
    }
}
