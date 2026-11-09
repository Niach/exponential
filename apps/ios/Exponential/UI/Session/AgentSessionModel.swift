import ExpCore
import ExpUI
import Foundation
import GRDB
import os
import UIKit

/// EXP-625: one line per lifecycle event (connect, dial, phase change, kick
/// decision, join-ack timeout, close). Payloads never go in: the whole point of
/// the activity channel is that it is scrubbed, and a log is not.
private let logger = Logger(subsystem: "at.exponential", category: "AgentSessionModel")

/// Viewer side of the steer ACTIVITY channel (EXP-32 — the session's raw
/// work; the frames of apps/steer-relay/src/protocol.ts). Since VAPP-43 it
/// rides eNet (Enet/SteerEnet.swift, fixtures/steer-enet.json): joins the
/// device that runs (or holds the journal of) the session with
/// {"t":"join","channel":"activity"}, and receives scrubbed {t:'activity',
/// event} frames (narration / tool headlines / questions / subagents /
/// permissions / worktree diffs). #EXP-1528: the sealed relay tunnel is eNet's
/// invisible FALLBACK (`SteerFeedCarriers`: stall, no feed or a drop), eNet
/// taking over again once it is back. Neither carrier = NO FEED: the existing
/// `starting` / ended-device-offline states.
///
/// EXP-1295 (Model v3): the steer channel is the LIVE layer only. A message is a
/// `messages` row (`sessions.send({sessionId})`), Stop is `sessions.stop`,
/// a queued message is revoked with `sessions.unqueue` — the database is the
/// bus. A question card still answers over ONE semantic `answer` frame.
/// Mirrors the Android AgentSessionViewModel.
@MainActor @Observable
final class AgentSessionModel {
    enum Phase: Equatable {
        case idle
        case connecting
        case live
        /// No device fed the session (no node, no endpoint, every endpoint
        /// refused, join timeout, or `no_such_session`) while the synced row
        /// is still alive. The model redials every ~3s until a device answers.
        case starting
        /// The session ended (`bye`). Feed retained, input hidden.
        case ended(detail: String?)
        /// Unexpected link loss. With `reconnecting` the model auto-redials on
        /// jittered exponential backoff (EXP-243 — no manual Reconnect
        /// button); false only for terminal states.
        case closed(detail: String?, reconnecting: Bool)
    }

    /// Log-only phase name (EXP-625). Detail strings stay out: they can quote a
    /// server error, and this log is not scrubbed.
    private static func describe(_ phase: Phase) -> String {
        switch phase {
        case .idle: return "idle"
        case .connecting: return "connecting"
        case .live: return "live"
        case .starting: return "starting"
        case .ended: return "ended"
        case let .closed(_, reconnecting): return reconnecting ? "closed(reconnecting)" : "closed"
        }
    }

    /// EXP-625: the phase as the pure revival rule sees it (ExpCore's
    /// SteerReconnectPolicy.revive). A non-reconnecting close and `ended` are
    /// both final: nothing is going to dial again.
    private var phaseKind: SteerPhaseKind {
        switch phase {
        case .idle: return .idle
        case .connecting: return .connecting
        // EXP-1487: a live feed that dropped holds `.live` on screen while it
        // redials silently; to the revival rule it is a reconnecting close.
        case .live: return dropAt == nil ? .live : .closedReconnecting
        case .starting: return .starting
        case .ended: return .final
        case let .closed(_, reconnecting): return reconnecting ? .closedReconnecting : .final
        }
    }

    private(set) var phase: Phase = .idle {
        didSet {
            guard oldValue != phase else { return }
            logger.info("phase \(Self.describe(oldValue), privacy: .public) -> \(Self.describe(self.phase), privacy: .public)")
            // EXP-724: nothing is compacting on a session that is over — the
            // `ended` edge that would have closed the strip is never coming.
            if case .ended = phase { clearLiveWork() }
            // FEED-26: the fallback "last activity" — a run that goes quiet
            // the moment it comes live still needs a clock to count from.
            if phase == .live { markActivity() }
        }
    }
    /// The feed stays visible while disconnected (closed/ended states) and is
    /// cleared ONLY by the device's `activity_reset` frame (EXP-249) — the device
    /// sends one to every activity viewer immediately before its join replay,
    /// so the client never has to guess when to wipe. Item shapes and the pure
    /// grouping/resolution rules live in ExpCore's AgentFeed.
    private(set) var feed: [AgentFeedItem] = [] {
        didSet {
            feedVersion &+= 1
            // FEED-26: any appended or replaced item counts as activity. The
            // event's own `at` never does — it is optional on the wire.
            if phase == .live { markActivity() }
            if !applyingBatch { reproject() }
        }
    }
    /// EXP-582: the O(feed) projections, derived ONCE per feed change. They
    /// used to be computed properties read from the view body — `rows` three
    /// times per render (the list, the tab strip and the focused tab), and
    /// `activeQuestionIds` once per question card plus the toolbar and the
    /// composer — so a join replay of a long history was one full walk of
    /// the feed per card per frame, and the main thread pinned at 100% for
    /// as long as the replay lasted.
    private(set) var rows: [AgentFeedRow] = []
    /// #EXP-1625: `rows` as the transcript draws them, one immutable entry
    /// per row (its identity + the gap above it), re-derived only when the
    /// rows moved. The view diffs these by id and gates each row on its own
    /// inputs, so nothing in the body walks or copies the feed.
    private(set) var transcript: [TranscriptEntry] = []
    /// #EXP-1625: the feed's length, for the follow pin (reading `feed` in
    /// the body tracked every feed write and copied the array per frame).
    private(set) var feedCount = 0
    /// #EXP-1625: the trailing unsettled tool call (`AgentFeed.liveToolRowId`
    /// without the phase gate, which the view applies).
    private(set) var openToolRowId: Int?
    /// #EXP-1625: the feed's tool calls as the session-card anchoring reads
    /// them (`AskDock.feedCalls`), derived per feed write, not per render.
    private(set) var feedCalls: [AskDock.FeedCall] = []
    /// EXP-783: there is transcript ABOVE the rendered window — rows the feed
    /// already holds, or a page only the device has. What the transcript's
    /// "Load earlier" affordance is gated on.
    private(set) var canLoadEarlier = false
    /// #EXP-1736 (transcript-grouping.json `fromStart`): the rendered
    /// window begins at the session's very first row — nothing above it in
    /// the feed, no older page on the device, no row trimmed off the front.
    /// The ONE origin rule reads the first prompt as the origin's only then.
    private(set) var transcriptFromStart = false
    /// The feed's byte cap dropped rows off its front (`trimFeed`): the
    /// start is gone for this viewer, so the origin never draws.
    @ObservationIgnored private var feedHeadTrimmed = false
    /// The device answered an older-page ask with nothing: the feed holds
    /// the journal's first row (unlike a publisher that cannot page at all).
    @ObservationIgnored private var historyReachedStart = false
    /// EXP-783: the id of the oldest RENDERED feed row — nil means the newest
    /// `AgentFeed.feedWindow` rows, `windowFromStart` the feed's own first row
    /// whatever it is. An id rather than an index, so an eviction or a replay
    /// swap cannot slide the window under the reader.
    @ObservationIgnored private var windowFrom: Int?
    /// EXP-795 — the anchor that means "the feed's first row": set when the
    /// reader asks for a page the feed does not hold yet, so a prepended page
    /// is inside the window the moment it lands. Below every real id (ids only
    /// ever count down at the front by a finite amount).
    private static let windowFromStart = Int.min
    private(set) var activeQuestionIds: Set<Int> = []
    /// EXP-788: the still-active question cards in feed order — what the
    /// composer's answer routing walks (a handful at most) instead of the feed.
    private var activeCards: [AgentQuestion] = []
    private(set) var subagents: [AgentSubagentRun] = []
    /// Per-card answer lock (EXP-249): a tap locks its card immediately, the
    /// desktop's `answer_ack` makes that permanent (and advances a stepper),
    /// and an unanswered optimistic lock expires so the card stays retryable.
    private(set) var answerTracker = AgentAnswerTracker()
    /// The most recent worktree diff — each one replaces the previous.
    private(set) var latestDiff: String?
    /// EXP-895/EXP-932: `latestDiff` parsed ONCE, memoised on its source —
    /// the Changes face, the file sheet and the Work screen's switcher totals
    /// all read this, so a diff edge costs one main-thread parse, not one per
    /// reader. Empty when there is no diff.
    @ObservationIgnored private var parsedDiffCache: (source: String, parsed: Diff.Parsed)?
    var parsedDiff: Diff.Parsed {
        guard let latestDiff else { return Diff.Parsed(files: []) }
        if let cache = parsedDiffCache, cache.source == latestDiff { return cache.parsed }
        let parsed = Diff.parse(latestDiff)
        parsedDiffCache = (latestDiff, parsed)
        return parsed
    }
    /// EXP-773: why an ENDED run has no transcript. The device that ran it
    /// replays its on-disk journal over eNet (VAPP-43 stored replay); when no
    /// device answers the join, the transcript is out of reach. Nil on a live
    /// session and once the transcript has arrived.
    enum HistoryState: Equatable {
        /// The machine that holds the journal did not answer. NOT terminal
        /// (VAPP-169): the model redials when that device flips online, on a
        /// wakeup and every `SteerEnet.staleMs` (`SteerOfflineRedial`).
        case deviceOffline
    }

    private(set) var history: HistoryState?
    /// EXP-724: a context compaction is running on the host agent — the
    /// composer grows an indeterminate strip for as long as this is non-nil,
    /// so the 10–170s of silence reads as work instead of a hang. Set by the
    /// `compaction` activity event and cleared by its `ended` edge, the replay
    /// swap, the end of the session, and a backstop timer.
    private(set) var compacting: AgentCompaction?
    /// EXP-848/850: the `turn` slot — whether the agent is inside a TURN right
    /// now (the engine's own edges, never a guess off the feed's shape) plus
    /// §5's caption inputs, the turn's start stamp and its output-token count.
    /// A latest-wins slot beside `compacting`, never a feed row. `.ended`
    /// until one arrives: an idle run must not pulse by default, and a
    /// publisher too old to emit them simply never reads as working.
    ///
    /// #EXP-1623 (Android #EXP-1521): NOT observed. A working agent re-sends
    /// this slot every second or so (its clock, tokens, `Writing…`), and
    /// every view that read it re-rendered on each tick, the whole transcript
    /// and the composer included. The one reader of the caption inputs is the
    /// working row's own once-a-second `TimelineView`; everything else reads
    /// `turnState`, which moves only on an edge.
    @ObservationIgnored private(set) var turn = AgentTurnSlot() {
        didSet { if turn.state != turnState { turnState = turn.state } }
    }
    /// EXP-848: the edge alone, which is what every working/pulse rule reads.
    private(set) var turnState: AgentTurnState = AgentTurnSlot().state
    /// EXP-850 §2: the background tasks the agent is running right now — a
    /// latest-wins slot (an empty list closes the strip above the composer).
    private(set) var backgroundTasks: [AgentBackgroundTask] = [] {
        didSet {
            // EXP-1324: a task the wire gave no `startedAt` counts from the
            // first time this client saw its id; a task that left the list
            // leaves the table (the list IS the current set).
            let now = Int64(Date().timeIntervalSince1970 * 1000)
            var seen: [String: Int64] = [:]
            for task in backgroundTasks {
                seen[task.id] = backgroundFirstSeen[task.id] ?? now
            }
            backgroundFirstSeen = seen
        }
    }
    /// EXP-1324: each subagent's start/end stamps (epoch ms, the edges' `at`);
    /// pruned to the subagents the feed still holds on every subagent edge.
    @ObservationIgnored private var subagentSpans: [String: (start: Int64?, end: Int64?)] = [:]
    @ObservationIgnored private var subagentSpansPruneAt = 32

    /// EXP-1324: a subagent's duration for its Trace badge — end − start once
    /// done, else the time since it started; nil without a start stamp.
    func subagentDurationMs(_ subagentId: String, done: Bool) -> Int? {
        guard let span = subagentSpans[subagentId], let start = span.start else { return nil }
        let end = done ? span.end : Int64(Date().timeIntervalSince1970 * 1000)
        guard let end else { return nil }
        return Int(max(0, end - start))
    }

    /// EXP-1324: first sight per background-task id (epoch ms).
    @ObservationIgnored private var backgroundFirstSeen: [String: Int64] = [:]
    /// EXP-1321: the Queue's two open states — the reader's toggle sticks for
    /// the session ("to do" open, "completed" collapsed by default).
    var queueTodoOpen = true
    var queueCompletedOpen = false
    /// EXP-927 §2c: the agent's own task list — a latest-wins slot (the FULL
    /// list in order; an empty one hides the block above the strip).
    private(set) var taskList: [AgentTaskListEntry] = []
    /// A settled `execute` call's output parsed as a test run, by CALL id —
    /// filled once when its `tool_update` lands, never re-parsed per render
    /// (the parser is seven regexes over the whole output). Keyed by the
    /// wire call id so a page renumbering the rows leaves it standing; pruned
    /// with the feed.
    private(set) var testResults: [String: TestResults] = [:]
    /// EXP-861: the messages the device is holding until the current turn or
    /// compaction ends — a latest-wins slot (the FULL queue, oldest first; an
    /// empty list closes the strip above the composer). Lives on the device,
    /// so a reconnecting or second viewer gets it from the join replay.
    private(set) var queued: [QueuedMessage] = []
    /// EXP-1321: the Queue's open waits — one per UNSETTLED `wait` call by
    /// call id (web/desktop count every open wait; two on one subject are
    /// two), derived ONCE per feed change in `reproject` (EXP-582: an O(feed)
    /// projection read from a view body is what pinned the main thread during
    /// a replay — and the Queue's `build` ticks every second).
    private(set) var openWaits: [TaskQueueInput.Wait] = []
    /// EXP-850 §3: the run's workflow cards, latest-wins per id in
    /// first-appearance order. Each one patches onto the `tool` row with the
    /// same id; it is never a feed row of its own.
    private(set) var workflows: [AgentWorkflow] = []
    /// FEED-26: when this run's feed last CHANGED while live, falling back to
    /// the moment the phase became live. What `staleActivityMinutes` counts
    /// from.
    /// #EXP-1623: not observed (it moves with every feed write); a reader
    /// picks it up with `activityNow`.
    @ObservationIgnored private(set) var lastActivityAt: Date?
    /// FEED-26: the clock `staleActivityMinutes` reads, re-stamped every 30s.
    /// Silence is the ABSENCE of frames, so without a tick nothing would ever
    /// redraw the header while a run goes quiet (the host-liveness pattern).
    private(set) var activityNow = Date()
    /// EXP-746: the agent's live configuration behind the composer chips —
    /// model, effort, mode, and the agent's own slash commands. Latest-wins
    /// STATE beside the feed, never a row: the device replays the newest
    /// `config_state` after its log, so a join and a reconnect both repaint
    /// from one frame.
    private(set) var sessionConfig: AgentSessionConfig?
    /// EXP-746: this run's context window and spend, as the engine last
    /// measured them. A different quantity from `agentUsage` (the machine's
    /// rate-limit windows), so it lives beside it and renders in its own
    /// block.
    private(set) var sessionUsage: AgentSessionUsage?
    /// EXP-1051: where that window actually GOES — the layers the device
    /// attributed before the first turn, a latest-wins slot beside
    /// `sessionUsage` (which is what gives them a scale). Nil = the device has
    /// published no layout, an empty array = a layout that attributed nothing.
    private(set) var sessionContextLayout: [ContextSegment]?
    /// EXP-784: the agent's rate-limit window, the fourth latest-wins slot.
    /// Nil = not limited (or cleared by an empty/`ok` status).
    private(set) var sessionRateLimit: AgentSessionRateLimit?
    /// The synced `sessions` row — flips to ended via Electric.
    private(set) var session: SessionEntity?
    /// EXP-1295: the session's ROOT row (the row itself on a root) — the
    /// chat's Guide lives there, and a `sessions_show` picture resolves
    /// against it.
    private(set) var root: SessionEntity?
    /// #EXP-1541: a session runs under this row (the device move's
    /// `queuedRoot` exception needs none).
    private(set) var hasSubSessions = false
    /// The session's display title (EXP-1341 `SessionTitle`, the same name
    /// its card, chip and row read) — the screen's title.
    private(set) var title: String?
    /// EXP-1310: the chat's title (the ROOT's `SessionTitle`) — the header's
    /// back row, ×4 (web `session-back`, Android `session-open-chat`).
    private(set) var chatTitle: String?
    /// EXP-549/550: the host machine as it presents right now — the LIVE
    /// `devices` row's label and whether that machine stopped heartbeating.
    private(set) var hostDevice = SessionHostDevice(label: nil, online: nil)
    /// EXP-484: how much of its rate-limit window the agent running THIS
    /// session has used, off the host machine's synced report. Nil whenever
    /// there is nothing honest to draw (`AgentUsagePresentation.sessionUsage`).
    private(set) var agentUsage: SessionAgentUsage?
    /// EXP-688: the host machine's sign-in status for THIS session's agent,
    /// for the Usage sheet's caption. Read-only visibility off the same synced
    /// row — nothing here holds or forwards a credential.
    private(set) var agentAccount: AgentAccount?
    /// EXP-909: WHICH login this session SPENDS — the synced
    /// `sessions.account` when it names one of the host's profiles, else the
    /// login whose email the machine reports for the agent, else its active
    /// one. Nil = the machine listed nothing: UNKNOWN, never a guess.
    private(set) var runAccount: AgentProfileUsageRow?
    /// EXP-909: the RESOLVED account's own rate-limit windows — the overlay's
    /// header block. The profile's own report, falling back to the machine's
    /// top-level `agentUsage[agent]` only when that profile is its ACTIVE
    /// login or the account is unknown. Not gated on freshness: old numbers
    /// dim and date themselves instead of hiding.
    private(set) var runUsage: AgentUsage?
    /// EXP-829/EXP-909: the command queue the overlay's one-shot usage refresh
    /// rides. Set by the session screen; nil = nothing is ever queued.
    var devicesApi: DevicesApi?
    /// VAPP-33: this install's eNet node, the PRIMARY steer carrier.
    /// Set by the session screen; nil = no node (the relay may still carry).
    @ObservationIgnored var steerEnet: SteerEnetCarrier?
    /// #EXP-1528: `steer.tunnelTicket`, the sealed relay fallback's ticket.
    /// Set by the session screen; nil = no relay leg (eNet only).
    @ObservationIgnored var steerTunnel: SteerTunnelTicketing?
    /// #EXP-1528: the connection analytics buffer (`connStats`).
    @ObservationIgnored var connStats: SteerConnStatsRecorder?
    /// The host machine as the devices shape presents it — the usage
    /// refresh's target.
    private(set) var hostSteerDevice: SteerDevice?
    /// A failed Stop, surfaced as a toast — cleared on each attempt. Success
    /// needs no local state: the turn's own edge is the confirmation.
    private(set) var stopError: String?
    /// S-3: a stop no device got (`delivered == false`), stamped per attempt
    /// so the toast fires each time.
    private(set) var stopNotDeliveredAt: Date?
    /// Why the last plain message failed to send — a toast, cleared on the
    /// next attempt (the draft stays put).
    private(set) var sendError: String?
    /// A message is out — uploads included (EXP-511): the composer dims its
    /// send button until the whole batch is out (`SessionComposerDraft`).
    var steerSending = false
    /// Why the last image-carrying send failed, shown under the thumbnail
    /// strip; cleared on the next attempt.
    var steerImageError: String?
    /// EXP-621: the composer draft — text and picked images — lives on the
    /// MODEL, not the view. It has to survive both a reconnect (the composer
    /// stays up while the socket is down, so a message typed mid-drop goes out
    /// when it returns) and navigating away and back, and the model is the only
    /// thing that outlives the screen (SteerSessionStore).
    ///
    /// EXP-802: the draft is an EDITOR, not a String, so the composer gets the
    /// `@` member / `#` issue / `:` emoji typeahead the comment composer has —
    /// all three need a caret to splice at, and a `TextField` binding has none.
    /// Exactly one text block (`MarkdownComposerField` keeps it that way).
    /// `draftText` / `trimmedDraft` and the whole send are
    /// `SessionComposerDraft`'s (THE session composer box, EXP-1295).
    let draftEditor = IssueEditorModel()
    /// EXP-511: images picked for the next steer message, shown as a strip
    /// above the input row until they are sent or removed.
    var pendingImages: [PendingSteerImage] = []
    /// Wave D: non-image files picked for the next steer message — the same
    /// strip, a file tile each, sent as `[<filename>](/api/attachments/<id>)`
    /// lines after the image embeds.
    var pendingFiles: [PendingSteerFile] = []

    /// EXP-772: the run's mode, as the agent advertises it. Nil until the
    /// first `config_state`, and on every run whose agent advertises no modes.
    ///
    /// EXP-790 made mode LAUNCH-TIME, so this is never a control any more — but
    /// EXP-847 keeps it as what the header's READ-ONLY Plan chip reads: a plan
    /// run has to SAY so, and an approved `ExitPlanMode` visibly clears it when
    /// the next `config_state` lands.
    var modeChip: AgentModeChip? {
        AgentFeed.modeChip(sessionConfig)
    }

    /// EXP-847: the run is in PLAN mode right now — the header chip's whole
    /// rule, mirrored ×4. Read off the latest `config_state` (the `plan` +
    /// one-other pair collapses into `planToggle`, a longer mode list is
    /// compared by id), so leaving plan mode clears the chip by itself. Never a
    /// control: EXP-790 keeps plan mode launch-time.
    var planModeActive: Bool {
        guard let chip = modeChip else { return false }
        if let toggle = chip.planToggle { return toggle.on }
        return sessionConfig?.currentMode == AgentFeed.planModeId
    }

    /// EXP-746: the id the catalog is keyed on — this run's agent, or, for an
    /// agent-less run that publishes a `config_state`, the external one that
    /// cannot name itself on the wire.
    var catalogAgent: String {
        SlashCommands.agentId(session?.cli, acp: sessionConfig != nil)
    }

    /// The composer's model pill: the live steer model, else the synced
    /// row's pick, else the host's default, else the contract defaults — so a
    /// chat with no live feed still names its model.
    /// `row` overrides `session` (a chat's root row arrives first).
    func composerModelValue(row: SessionEntity? = nil) -> String {
        let row = row ?? session
        return LaunchVocabulary.composerModel(
            live: sessionConfig?.modelValue,
            rowModel: row?.model,
            cli: row?.cli,
            launchDefaults: hostSteerDevice?.launchDefaults
        )
    }

    /// The catalog command the draft would SEND as, if any — what the confirm
    /// gate reads before a `/clear` goes out.
    var pendingSlashCommand: SlashCommand? {
        SlashCommands.command(for: draftText, agent: catalogAgent)
    }

    /// Over as far as this client can tell: an explicitly `ended` row, or a
    /// row that VANISHED. The model is always constructed with a real row, so
    /// nil means it was deleted (stale rows get swept) or left this client's
    /// sync scope — either way it can never report `ended` itself, and every
    /// retry loop exits on this, so treating nil as "still running" would keep
    /// them dialing forever.
    var sessionEnded: Bool {
        guard let session else { return true }
        return !session.isLive
    }

    /// EXP-621: the session is finished as far as this screen is concerned —
    /// the device said `bye` or the synced row ended/vanished. The ONLY state
    /// that retires the composer: every other one (connecting, starting,
    /// reconnecting) keeps it on screen with sending disabled, so a draft is
    /// never lost to a hiccup. Also what SteerSessionStore reaps on.
    var isOver: Bool {
        if case .ended = phase { return true }
        return sessionEnded
    }

    /// Whether a steer frame can go out right now (a question's `answer`, a
    /// mode switch) — a live link on a live session. `connected` matters
    /// beyond the phase: a silent stale redial deliberately keeps
    /// `phase == .live` while the link is briefly down.
    var canSteer: Bool { phase == .live && connected && !sessionEnded }

    /// EXP-1295: this viewer may post to the session and Stop it — a live row
    /// that is the caller's own or SHARED with the team (Model v3's one
    /// visibility rule). Display gating only — the server re-decides it.
    var mayPost: Bool {
        guard let session, !sessionEnded else { return false }
        if session.shared { return true }
        guard let currentUserId else { return false }
        return session.userId == currentUserId
    }

    /// Whether a message can go out right now. Messages ride the database
    /// (`sessions.send`), so a dropped socket does not stop one — an idle
    /// session resumes on it.
    var canSend: Bool { mayPost && !steerSending }

    /// EXP-550: the host machine is offline while the session is live — it
    /// is PAUSED, not ended (it resumes when the machine comes back), so the
    /// viewer shows that instead of an endless "waiting for the live stream".
    var hostDeviceOffline: Bool {
        hostDevice.offline && !sessionEnded
    }

    /// Whether this viewer may Stop the agent's turn (`sessions.stop`).
    var canStop: Bool { mayPost }

    // MARK: EXP-849 — the usage wall's account switch

    /// The host machine's signed-in logins for this session's agent, each
    /// with its refusal (`SessionAccountSwitch.options`).
    var accountSwitchOptions: [SessionAccountSwitch.Option] {
        guard let session else { return [] }
        return SessionAccountSwitch.options(
            accounts: hostSteerDevice?.agentAccounts,
            usage: hostSteerDevice?.agentUsage,
            launchDefaults: hostSteerDevice?.launchDefaults,
            cli: session.cli,
            account: session.account
        )
    }

    /// The run-level refusal (agent → machine → turn), or nil.
    var accountSwitchBlocked: String? {
        SessionAccountSwitch.blockedReason(
            agent: SessionAccountSwitch.agent(
                cli: session?.cli,
                defaultAgent: hostSteerDevice?.launchDefaults?.defaultAgent
            ),
            deviceOnline: hostDevice.online == true,
            busy: session?.busy == true
        )
    }

    /// A switch is out — the wall's control dims until it lands.
    private(set) var switchingAccount = false
    /// Why the last switch failed — a toast, cleared on the next attempt.
    private(set) var switchError: String?

    /// EXP-849: continue this session on `option`'s login — ONE
    /// `sessions.send({sessionId, account})`, no text. The host resumes the
    /// SAME row on it between turns; the synced `account` is the confirmation.
    func switchAccount(to option: AccountOption) async {
        guard mayPost, !switchingAccount else { return }
        switchError = nil
        switchingAccount = true
        defer { switchingAccount = false }
        do {
            try await sessionsApi.send(
                accountId: accountId,
                SessionAccountSwitch.sendInput(sessionId: sessionId, option: option)
            )
        } catch {
            switchError = error.trpcUserMessage
        }
    }

    /// #EXP-1535 (composer-menu.json `account`): the "+" Account page of
    /// this session's composer — THE usage wall's switch above as the
    /// AccountPicker rows — while the row is live, its device online and the
    /// viewer steers it; nil = no row. A run-level refusal disables every
    /// row, a login's own refusal its row; the page is never hidden for them.
    var composerAccountMenu: SessionComposerFooter.AccountMenu? {
        guard let session, session.isLive, hostDevice.online == true,
              SessionAccountSwitch.canSteer(
                  shared: session.shared, userId: session.userId, currentUserId: currentUserId
              )
        else { return nil }
        let options = accountSwitchOptions
        // No login reported for the session's agent = no row (web, desktop).
        guard !options.isEmpty else { return nil }
        let blocked = accountSwitchBlocked
        return SessionComposerFooter.AccountMenu(
            options: options.map(\.account),
            current: options.first(where: \.current)?.account,
            disabledKeys: Set(
                options.filter { blocked != nil || $0.blockedReason != nil }.map(\.account.key)
            ),
            onPick: { [weak self] key in
                guard let self, self.accountSwitchBlocked == nil,
                      let picked = options.first(where: { $0.account.key == key }),
                      picked.blockedReason == nil
                else { return }
                Task { await self.switchAccount(to: picked.account) }
            }
        )
    }

    // MARK: EXP-1252 — move the session to another device

    /// The launch composer's machines for the session's team (own +
    /// team-shared), off the same synced rows the host read-out uses.
    private var moveCandidateDevices: [SteerDevice] {
        guard let session else { return [] }
        return DeviceQueries.compose(
            rows: deviceRows,
            users: mentionUserRows,
            teamId: session.teamId,
            userId: currentUserId,
            now: Date()
        )
    }

    /// The composer's Device pin (before Model), or nil = hidden (agents, no
    /// row yet). Rules + copy: `SessionDeviceMove`.
    var devicePin: SessionDevicePin? {
        guard let session,
              SessionDeviceMove.pinVisible(kind: sessionKindOf(agentId: session.agentId))
        else { return nil }
        let devices = moveCandidateDevices
        let agent = SessionAccountSwitch.agent(
            cli: session.cli,
            defaultAgent: hostSteerDevice?.launchDefaults?.defaultAgent
        )
        let rows = SessionDeviceMove.rows(
            devices: devices,
            currentDeviceId: session.deviceId,
            sessionTeamId: session.teamId,
            sessionShared: session.shared,
            agent: agent,
            queuedRoot: SessionDeviceMove.queuedRoot(
                status: session.status,
                parentId: session.parentId,
                hasSubSessions: hasSubSessions
            ),
            starterId: session.userId,
            viewerId: currentUserId
        )
        let current = devices.first { $0.deviceId == session.deviceId } ?? hostSteerDevice
        let currentName = current.map(LaunchVocabulary.deviceName)
            ?? hostDevice.label ?? session.deviceId
        let canSteer = SessionAccountSwitch.canSteer(
            shared: session.shared, userId: session.userId, currentUserId: currentUserId
        )
        return SessionDevicePin(
            label: currentName,
            icon: current.map { DeviceIconDisplay.iconName(for: $0) },
            rows: rows.map(DevicePickerDevice.init(moveRow:)),
            currentDeviceId: session.deviceId,
            blockedReason: SessionDeviceMove.blockedReason(canSteer: canSteer, ended: !session.isLive),
            sending: movingDevice,
            onPick: { [weak self] deviceId in
                Task { await self?.moveDevice(to: deviceId) }
            }
        )
    }

    /// A move request is out — the pin stays put until it lands.
    private(set) var movingDevice = false
    /// Why the last move request failed — a toast, cleared on each attempt.
    private(set) var moveError: String?

    /// EXP-1252: ONE `sessions.send({sessionId, deviceId})` to another
    /// device; the current one does nothing (NEW-8: nothing is pending on the
    /// row, the server moves the session at once).
    func moveDevice(to deviceId: String) async {
        guard let session, !movingDevice,
              let input = SessionDeviceMove.sendInput(
                  sessionId: session.id,
                  pick: SessionDeviceMove.pick(deviceId: deviceId, currentDeviceId: session.deviceId)
              )
        else { return }
        moveError = nil
        movingDevice = true
        defer { movingDevice = false }
        do {
            try await sessionsApi.send(accountId: accountId, input)
        } catch {
            moveError = error.trpcUserMessage
        }
    }

    /// Recompute the cached projections (ids of the still-answerable question
    /// items, the collapsed render rows, the subagent runs) — the pure rules
    /// live in ExpCore's AgentFeed, mirroring Android.
    /// EXP-783 — the index of the oldest feed row the view renders.
    ///
    /// The feed keeps the whole run; the window is the newest
    /// `AgentFeed.feedWindow` rows, moved upward (never downward) by the
    /// reader reaching the top. Anchored on a row ID rather than an index, so
    /// a byte-budget eviction or a replay swap cannot slide it.
    private var windowStart: Int {
        let tail = max(0, feed.count - AgentFeed.feedWindow)
        guard let from = windowFrom else { return tail }
        guard let at = feed.firstIndex(where: { $0.id >= from }) else { return tail }
        return min(at, tail)
    }

    /// EXP-783 — pull one more page of the run into the view: more of the feed
    /// if there is any, else — with the window pinned to the FRONT so the page
    /// lands inside it (EXP-795) — the page below it from the device's
    /// journal. `false` when there is nothing left to load.
    @discardableResult
    func loadEarlier() -> Bool {
        let start = windowStart
        if start > 0 {
            windowFrom = feed[max(0, start - AgentFeed.feedWindowStep)].id
            reproject()
            return true
        }
        windowFrom = Self.windowFromStart
        return requestOlderPage()
    }

    /// EXP-795 — the reader is back at the newest rows (web/Android/desktop
    /// parity): release the window so it slides with the stream again instead
    /// of growing without bound behind a pin nobody is reading at.
    func noteAtBottom(_ atBottom: Bool) {
        guard atBottom, windowFrom != nil else { return }
        windowFrom = nil
        reproject()
    }

    /// EXP-783 — one `history_page` ask, at most one in flight.
    ///
    /// A viewer that joined a long-running session holds only the device's
    /// replay TAIL (`truncated` on `activity_synced` is how it knows), and the
    /// pages below it exist only in the device's journal.
    private func requestOlderPage() -> Bool {
        guard historyRequest == nil, !historyExhausted, historyTruncated else { return false }
        guard let before = oldestSeq, before > 0 else {
            historyExhausted = true
            // Seq 0 on screen = the journal's first row is held.
            if oldestSeq == 0 { historyReachedStart = true }
            canLoadEarlier = false
            if !applyingBatch { reproject() }
            return false
        }
        historyRequests += 1
        let requestId = "p\(historyRequests)"
        guard connected else { return false }
        send(frame: [
            "t": "history_page",
            "requestId": requestId,
            "beforeSeq": before,
            "limit": AgentFeed.historyPageLimit,
        ])
        historyRequest = requestId
        return true
    }

    /// EXP-783 — PREPEND one older page, oldest first.
    ///
    /// The page is transcript from BELOW everything on screen, so its rows are
    /// spliced in front: every visible row keeps its id (its SwiftUI
    /// identity), its answer state and its position. A page overlapping what
    /// is already held is trimmed against `oldestSeq` — a re-asked page must
    /// never double the transcript. Ignored while a replay is staging: the
    /// replay is authoritative and is about to decide what the prefix even is.
    private func prependPage(_ events: [SteerWireFrame.Activity], seqs: [Int]) {
        guard !isStaging, !events.isEmpty else { return }
        // EXP-848: the latest-wins turn slot must not be rewritten by a page
        // of OLD transcript folded through the live reducer.
        prependingPage = true
        defer { prependingPage = false }
        let oldest = oldestSeq
        // Fold the page through the SAME reducer the live stream uses, over a
        // scratch feed, so merging and upserts behave identically.
        let savedFeed = feed
        let savedBytes = feedBytes
        let savedNextId = nextEventId
        // The scratch fold numbers from zero, which would collide with the
        // sequence table's real entries — it gets its own table.
        let savedSeqs = seqById
        feed = []
        feedBytes = 0
        nextEventId = 0
        seqById = [:]
        let nested = applyingBatch
        applyingBatch = true
        for (index, event) in events.enumerated() {
            let seq = index < seqs.count ? seqs[index] : nil
            if let seq, let oldest, seq >= oldest { continue }
            currentSeq = seq
            handleActivityEvent(event)
        }
        currentSeq = nil
        applyingBatch = nested
        let page = feed
        let pageSeqs = seqById
        feed = savedFeed
        feedBytes = savedBytes
        nextEventId = savedNextId
        seqById = savedSeqs
        guard !page.isEmpty else {
            historyExhausted = true
            historyReachedStart = true
            canLoadEarlier = false
            if !applyingBatch { reproject() }
            return
        }
        // The prepended rows take ids BELOW every id on screen, so ordering by
        // id stays the ordering of the transcript.
        let base = (savedFeed.first?.id ?? page.count) - page.count
        var renumbered: [AgentFeedItem] = []
        renumbered.reserveCapacity(page.count)
        for (offset, item) in page.enumerated() {
            let id = base + offset
            if let seq = pageSeqs[item.id] { seqById[id] = seq }
            renumbered.append(item.withId(id))
        }
        feed = renumbered + savedFeed
        feedBytes += renumbered.reduce(0) { $0 + AgentFeed.itemBytes($1) }
        trimFeed()
        if !applyingBatch { reproject() }
    }

    private func reproject() {
        let start = windowStart
        // #EXP-1623 (Android #EXP-1521): every flush lands here, a live
        // `turn` tick's included, and a projection re-assigned to an EQUAL
        // value still invalidates every view that reads it (the transcript,
        // the Dock, the composer). Each one is written only when it moved.
        let canLoad = AgentFeed.canLoadEarlier(
            windowStart: start, historyTruncated: historyTruncated,
            historyExhausted: historyExhausted, connected: connected
        )
        if canLoad != canLoadEarlier { canLoadEarlier = canLoad }
        let fromStart = start == 0 && (!historyTruncated || historyReachedStart) && !feedHeadTrimmed
        if fromStart != transcriptFromStart { transcriptFromStart = fromStart }
        let workflowIds = Set(workflows.map(\.id))
        let key = ProjectionKey(feedVersion: feedVersion, windowStart: start, workflowIds: workflowIds)
        guard key != projectedKey else { return }
        projectedKey = key
        if hasFeed == feed.isEmpty { hasFeed = !feed.isEmpty }
        // EXP-850 §3: a workflow's tool row never collapses into a "N tool
        // calls" run (the card renders in its place) and its agents' runs are
        // nested INSIDE the card, so the transcript projection drops them.
        let nextRows = AgentFeed.rows(feed, from: start, workflowIds: workflowIds)
        if nextRows != rows {
            rows = nextRows
            transcript = TranscriptEntry.entries(nextRows, previous: transcript)
        }
        if feed.count != feedCount { feedCount = feed.count }
        let openTool = AgentFeed.liveToolRowId(feed)
        if openTool != openToolRowId { openToolRowId = openTool }
        let calls = AskDock.feedCalls(feed)
        if calls != feedCalls { feedCalls = calls }
        // The runs list keeps the workflow agents — the card looks its own
        // agents' duplicate warnings up here, and (EXP-1225) they are tabs.
        let nextSubagents = AgentFeed.subagents(feed, from: start)
        if nextSubagents != subagents { subagents = nextSubagents }
        let nextWaits = feed.compactMap { item -> TaskQueueInput.Wait? in
            guard case let .tool(id, name, detail, _, callId, kind, settled, _, _, _, _, _, _) = item,
                  kind == AgentFeed.toolKindWait, !settled else { return nil }
            return TaskQueueInput.Wait(
                id: callId ?? "\(id)",
                subject: (detail?.isEmpty == false) ? detail! : name
            )
        }
        if nextWaits != openWaits { openWaits = nextWaits }
        let active = AgentFeed.activeQuestionIds(feed)
        if active != activeQuestionIds { activeQuestionIds = active }
        activeCards = feed.compactMap { item in
            guard let question = item.question, active.contains(question.id) else { return nil }
            return question
        }
        let newest = Self.newestAgentMessageKey(feed)
        if newest != newestAgentMessageKey { newestAgentMessageKey = newest }
    }

    /// #EXP-1623: what the O(feed) half of `reproject` is a function of.
    private struct ProjectionKey: Equatable {
        let feedVersion: Int
        let windowStart: Int
        let workflowIds: Set<String>
    }

    @ObservationIgnored private var projectedKey: ProjectionKey?
    /// #EXP-1623: bumped by every feed write, so a flush that wrote none (a
    /// live tick) skips the projections' walk.
    @ObservationIgnored private var feedVersion = 0

    /// #EXP-1623: whether the feed holds a row, observed apart from the feed
    /// itself, so the Dock and the empty states never re-render per row.
    private(set) var hasFeed = false
    /// #EXP-1623: a join replay (or a stored one) has committed at least
    /// once, so the latest-wins slots are the device's, not a guess.
    private(set) var replayed = false

    /// The composer's placeholder. EXP-820: always the generic prompt — the
    /// composer HIDES while a card is pending (`cardPending`), and a card's
    /// free answer is an inline field on the card itself.
    var composerPlaceholder: String { AgentFeed.composerPlaceholder }

    /// EXP-820: a card is waiting on THIS steerer — live, not ended, and an
    /// unresolved question card in the feed. N-12: it waits in THE Dock; the
    /// composer stays.
    var cardPending: Bool {
        phase == .live && !sessionEnded && !activeQuestionIds.isEmpty
    }

    /// EXP-848: the agent is actively working — the ONE predicate this screen
    /// has (the view used to carry a second, looser copy that ignored the
    /// compaction gate and the usage wall, so the trailing "Working…" row and
    /// the Stop button disagreed). The rule itself is `AgentFeed.working`,
    /// mirrored ×4: a turn is open and nothing is waiting on a human, walling
    /// the run or folding its context away.
    ///
    /// Drives the transcript's trailing "Working…" row AND the empty
    /// composer's Stop glyph (EXP-790).
    var agentWorking: Bool {
        AgentFeed.working(
            live: phase == .live,
            sessionEnded: sessionEnded,
            turnState: turnState,
            awaitingInput: !activeQuestionIds.isEmpty,
            needsInput: session?.needsInput == true,
            // EXP-804: "blocked" is whatever the badge considers blocked — the
            // raw jsonb text may be present but unreadable.
            blocked: AgentUsagePresentation.parseBlocked(session?.blocked) != nil,
            compacting: compacting != nil
        )
    }

    // MARK: - Workflows and the bottom strip (EXP-850)

    /// The card a tool row carries, looked up by its call id — the §3 patch:
    /// the `workflow` event renders IN PLACE of the `Workflow` tool row.
    func workflow(for callId: String?) -> AgentWorkflow? {
        guard let callId else { return nil }
        return workflows.first { $0.id == callId }
    }

    /// §7: the NEWEST still-running card — what the working caption and the
    /// header speak for while a workflow runs.
    var runningWorkflow: AgentWorkflow? { AgentFeed.runningWorkflow(workflows) }

    /// §4: the subagent run behind one workflow agent, for the card's nested
    /// preview and its duplicate warning.
    func subagentRun(agentId: String?) -> AgentSubagentRun? {
        guard let agentId else { return nil }
        return subagents.first { $0.subagentId == agentId }
    }

    /// EXP-1321/1324: THE Queue at `now` (`fixtures/task-queue.json`): the
    /// task list, the background tasks and the open waits; nil = no block.
    /// Gone once the run is over, like the strip it replaced.
    func taskQueue(now: Date) -> TaskQueue? {
        guard !isOver else { return nil }
        return TaskQueue.build(TaskQueueInput(
            tasks: taskList.map { WorkCardTask(content: $0.content, status: $0.status.rawValue) },
            backgroundTasks: backgroundTasks.map {
                TaskQueueInput.BackgroundTask(
                    id: $0.id, kind: $0.kind, description: $0.description,
                    startedAt: $0.startedAt ?? backgroundFirstSeen[$0.id]
                )
            },
            waits: openWaits,
            now: Int64(now.timeIntervalSince1970 * 1000)
        ))
    }

    /// EXP-1323: the newest agent prose in the MAIN lane (its wire message
    /// id, else the row id) — the session view re-marks the session read
    /// whenever this moves while it is open (Android `ChatScreen` keys its
    /// read mark the same way).
    /// #EXP-1623: stored, re-derived by `reproject`; the session view's
    /// read mark watches it, and a computed walk read there re-rendered the
    /// whole view on every feed write.
    private(set) var newestAgentMessageKey: String?

    private static func newestAgentMessageKey(_ feed: [AgentFeedItem]) -> String? {
        for item in feed.reversed() {
            if case let .narration(id, _, messageId, subagentId) = item, subagentId == nil {
                return messageId ?? "\(id)"
            }
        }
        return nil
    }

    /// §5: the trailing working row's caption at `now` — the turn's verb (or
    /// the running workflow's caption, or EXP-1501's `Writing {tool}…`) with
    /// its duration and token count. The
    /// row itself ticks the clock, so this stays pure.
    func workingCaption(now: Date) -> String {
        AgentFeed.workingCaption(
            startedAt: turn.startedAt, tokens: turn.tokens, now: now,
            workflow: runningWorkflow, writing: turn.writing
        )
    }

    /// Questions whose answer is out — sent (optimistic lock) or confirmed
    /// (`answer_ack`). What a stepper card advances on (web parity: advance on
    /// send; the 5s no-ack expiry rolls the step back).
    var answeredQuestionIds: Set<String> { answerTracker.lockedKeys }

    /// Whether a card is locked against further taps (sent-and-unconfirmed, or
    /// confirmed by the desktop).
    func isAnswerLocked(_ lockKey: String) -> Bool { answerTracker.isLocked(lockKey) }

    /// The labels this client picked for a locked card (EXP-588) — what an
    /// answered stepper step shows before `question_resolved` fills `answers`.
    func localAnswerSummary(_ lockKey: String) -> String? {
        answerTracker.answerSummary(lockKey)
    }

    /// An answer went out for this card but the desktop hasn't confirmed
    /// injecting it yet.
    func isAnswerPending(_ lockKey: String) -> Bool { answerTracker.isPending(lockKey) }

    /// The last answer for this card expired unconfirmed — the card is
    /// answerable again and shows a retry hint (EXP-334, web parity).
    func isAnswerFailed(_ lockKey: String) -> Bool { answerTracker.isFailed(lockKey) }

    /// Live but blocked on a trailing question/plan — the session is waiting
    /// for a human answer, not stuck (EXP-97).
    var awaitingInput: Bool { phase == .live && !activeQuestionIds.isEmpty }

    /// FEED-26: whole minutes this LIVE run's feed has said nothing, once past
    /// `AgentFeed.staleActivityAfter` — what the header prints as `No activity
    /// for 27 min` instead of a healthy-looking "Live". Nil in every state
    /// that ALREADY explains the silence: a paused host (its own caption), a
    /// trailing question or plan card (the run is blocked on a human, not
    /// stuck) and a running compaction, which is work that emits no frames.
    var staleActivityMinutes: Int? {
        guard phase == .live, !hostDeviceOffline, !awaitingInput, compacting == nil else {
            return nil
        }
        return AgentFeed.staleActivityMinutes(since: lastActivityAt, now: activityNow)
    }

    private let accountId: String
    private let sessionId: String
    private let currentUserId: String?
    private let sessionsApi: SessionsApi
    private let attachmentsApi: AttachmentsApi
    private let issuesApi: IssuesApi
    private let db: DatabaseManager
    /// EXP-892 — the `#` menu's server half, held so its debounce outlives the
    /// keystroke that armed it (the composer model keeps the same handle).
    @ObservationIgnored private var issueRefAugmentor: IssueRefAugmentor?

    /// VAPP-33 / #EXP-1528: the feed over its two carriers (eNet first, the
    /// sealed relay as fallback): every client frame goes out on the active
    /// one, its frames feed the reducer.
    @ObservationIgnored private var carriers: SteerFeedCarriers?
    @ObservationIgnored private var staleTask: Task<Void, Never>?
    /// #EXP-1528: this open's `steer.view` analytics and its settle timer.
    @ObservationIgnored private var viewStats: SteerViewStats?
    @ObservationIgnored private var settleTask: Task<Void, Never>?
    /// EXP-796: an open viewer socket is one of `canLoadEarlier`'s inputs, so
    /// the projection re-derives on every flip.
    private var connected = false {
        didSet { if oldValue != connected, !applyingBatch { reproject() } }
    }
    private var stopped = false
    private var sawEnd = false
    private var retryStarting = false
    private var endDetail: String?
    private var nextEventId = 0
    /// EXP-783: the running weight of `feed`, so the byte budget is an integer
    /// compare per append rather than a walk.
    private var feedBytes = 0
    /// #EXP-1623: how many `thought` rows may still be running, so a turn
    /// that ends walks the feed to settle them only when one is (a replay
    /// carries a turn edge per turn). An over-count only costs that walk.
    @ObservationIgnored private var openThoughts = 0
    /// EXP-783: the publisher's wire sequence per feed row, by row id. A side
    /// table rather than a case field on `AgentFeedItem`: nothing that renders
    /// a row cares, and every pattern match over the enum would have had to
    /// change. It is the only monotonic anchor on the wire — what lets a join
    /// replay be spliced onto a prefix already on screen, and what an
    /// older-page request is addressed relative to.
    private var seqById: [Int: Int] = [:]
    /// The wire sequence of the event being folded in right now, so `append`
    /// can stamp it without passing it through every branch.
    @ObservationIgnored private var currentSeq: Int?
    /// EXP-783: the `history_page` request in flight — a chunk for any other
    /// id is not ours; whether the device said its log is a TAIL (so there IS
    /// older transcript to ask the device for); and whether a page came back
    /// with nothing new (stop asking).
    @ObservationIgnored private var historyRequest: String?
    @ObservationIgnored private var historyRequests = 0
    private var historyTruncated = false
    private var historyExhausted = false
    /// Locally-echoed sent messages awaiting their transcript-derived
    /// `user_message` event (EXP-78 dedupe).
    private var recentEchoes: [AgentEcho] = []
    private var retryTask: Task<Void, Never>?
    /// Consecutive failed reconnect dials — indexes the backoff curve; reset
    /// on a successful (live) connection and on an explicit connect().
    private var reconnectAttempts = 0
    /// #EXP-1678: consecutive NO FEED redials (`fallback.startingRetry`);
    /// a feed resets it (`attachFeed`).
    private var startingRetries = 0
    /// EXP-1487 LOST BANNER: when a feed that was joined dropped (nil = no
    /// drop pending), and how many redials since then failed. A successful
    /// join resets both (`attachFeed`).
    private var dropAt: Date?
    private var failedRedialsSinceDrop = 0
    /// Monotonic dial id — each dial() invalidates the ones before it, so a
    /// dial that was superseded mid-await (a foreground connect() racing a
    /// fired backoff retry, EXP-243) can't install a second socket or stomp
    /// the winner's phase.
    private var dialGeneration = 0
    /// EXP-625: a dial is between its entry and its outcome (joined, no
    /// feed, or a superseding dial). Together with an armed
    /// `retryTask` this is `dialActive`, the ONE thing a wake signal gates on:
    /// the phase lies (a wedged dial sits at `.connecting` forever), a live
    /// dial does not.
    private var dialInFlight = false
    /// When the current dial started, and when the last frame arrived on it.
    /// `lastFrameAt == nil` means the device has not answered this dial's
    /// join (the eNet joiner bounds that wait with `joinTimeoutMs`).
    private var dialStartedAt: Date?
    @ObservationIgnored private var lastFrameAt: Date?
    private var sessionObservationTask: Task<Void, Never>?
    /// EXP-549/550: the synced `devices` rows behind `hostDevice`, plus the
    /// clock that re-derives offline-ness (GRDB only re-fires on WRITES, and
    /// a machine going quiet writes nothing).
    private var deviceObservationTask: Task<Void, Never>?
    private var deviceLivenessTask: Task<Void, Never>?
    /// FEED-26: the 30s tick that re-derives `staleActivityMinutes` — see
    /// `activityNow`.
    private var activityClockTask: Task<Void, Never>?
    /// EXP-656: wakes when our own `devices` shape completes a poll. Presence
    /// is only as current as that cursor — after a suspension the rows still
    /// carry the pre-sleep `last_seen_at`, and this is the edge that turns
    /// "we don't know" back into knowledge.
    private var deviceFreshnessTask: Task<Void, Never>?
    private var deviceRows: [DeviceEntity] = []
    /// EXP-802: the rows behind the composer's @-mention vocabulary — every
    /// synced user plus the membership rows that scope them to THIS run's team.
    private var mentionObservationTask: Task<Void, Never>?
    private var mentionUserRows: [UserEntity] = []
    private var mentionMemberRows: [TeamMemberEntity] = []
    /// One expiry timer PER locked card (EXP-334) — a single shared task used
    /// to be cancelled by every newer lock, so several pending locks then all
    /// expired together and the stepper rolled back more than one step.
    private var answerExpiryTasks: [String: Task<Void, Never>] = [:]
    /// EXP-724: the backstop behind an open compaction strip — armed on
    /// `started`, cancelled by `ended` and by every clear.
    private var compactionTimeoutTask: Task<Void, Never>?
    /// #EXP-1538: when the open compaction started (epoch ms) — carried into
    /// the last-known Dock slots so a seeded strip keeps its backstop.
    @ObservationIgnored private var compactingSince: Double?
    /// EXP-582: inbound steer frames waiting for the next flush. A receive
    /// callback that hops to the main actor ONCE PER FRAME, and with every
    /// frame mutating `feed` directly each one bought a full SwiftUI render
    /// plus a `scrollTo` — a join replay of a long history (the device replays
    /// its whole activity log frame by frame) froze the UI and heated the
    /// device for the duration. Frames now queue here and are applied in one
    /// batch per flush, so the replay costs one render per batch.
    /// #EXP-1623: each with the dial generation it arrived on, because a
    /// burst parses off main and a redial can land in between.
    @ObservationIgnored private var pendingFrames: [(text: String, generation: Int)] = []
    @ObservationIgnored private var flushScheduled = false
    /// Set while a flush applies its batch — the projections are rebuilt once
    /// at the end instead of after every frame.
    @ObservationIgnored private var applyingBatch = false
    /// EXP-848: set while `prependPage` folds an OLDER page through the live
    /// reducer — every latest-wins slot (turn, config, usage, rate limit, the
    /// compaction strip) is about the run's PRESENT, so a page from the far end
    /// of the transcript must not touch any of them. Web does the same by
    /// saving and restoring those slots around its fold.
    @ObservationIgnored private var prependingPage = false
    /// EXP-656: the activity events of an in-flight join replay. nil = not
    /// staging; non-nil (even empty) = the visible feed is frozen and every
    /// `activity` frame buffers here until the replay commits in ONE pass.
    @ObservationIgnored private var stagedFrames: [(activity: SteerWireFrame.Activity, seq: Int?)]?
    @ObservationIgnored private var stagingStartedAt: Date?
    @ObservationIgnored private var lastStagedFrameAt: Date?
    /// The one task that watches an in-flight staging window (quiet timeout +
    /// hard cap, both decided by SteerReplayStaging.shouldCommit).
    @ObservationIgnored private var stagingWatcherTask: Task<Void, Never>?
    /// Messages this client sent WHILE a replay was staging: the replay
    /// predates them, so the commit re-appends whatever it didn't carry back.
    @ObservationIgnored private var stagedLocalEchoes: [String] = []

    /// How long an optimistic answer lock holds without an `answer_ack` or a
    /// `question_resolved` — long enough to cover a slow desktop injection,
    /// short enough that a lost frame doesn't strand the card. Web/Android
    /// parity (ANSWER_ACK_TIMEOUT_MS, EXP-249). Derived from the desktop's
    /// worst-case ack budget (EXP-347): ANSWER_RETRY_TTL 4s + ANSWER_SETTLE 2s
    /// + PLAN_SUBMIT_PROBE 0.5s + ~1.5s tick/relay margin — move all three in
    /// lockstep.
    private static let answerLockSeconds: Double = 8
    /// EXP-656: a join replay is STAGED, not applied — see SteerReplayStaging.
    /// The device's `activity_synced` marker ends it; these bound the fallback
    /// for a publisher-driven republish that carries no marker. Quiet: the
    /// replay arrives as one burst, so 400ms of silence means it is over. Cap:
    /// a stalled republish commits what it has rather than holding the buffer.
    /// Android parity (SteerTimings.replayQuietMs / replayMaxMs).
    private static let replayQuietSeconds: Double = 0.4
    private static let replayMaxSeconds: Double = 3
    /// How often the staging watcher re-asks the pure rule whether the replay
    /// is over — fine enough that the quiet window and the cap both land close
    /// to their nominal times, coarse enough to be one task for the window
    /// instead of one per replayed frame.
    private static let replayWatchSeconds: Double = 0.1
    /// Auto-reconnect backoff after an unexpected drop (EXP-243): jittered
    /// exponential 3s→30s, mirroring the web viewer's starting retry — the
    /// jitter desyncs a herd of viewers all foregrounding at once.
    private static let reconnectBaseSeconds: Double = 3
    private static let reconnectMaxSeconds: Double = 30
    /// A nominally live link that has delivered nothing for this long is
    /// suspect, so a wake signal redials it silently rather than trusting the
    /// phase. The device sends every joined viewer a `keepalive` frame every
    /// `SteerEnet.keepaliveMs` (EXP-648), so this is more than one missed tick: an agent parked on a
    /// question or a plan approval produces no frames of its own, and must
    /// not read as a dead socket.
    private static let liveStaleSeconds: Double = 45
    /// The `starting` state's caption: no device feeds a live row yet.
    private static let startingDetail = "The live stream isn't up yet. The desktop may still be connecting."
    /// Shown when the session's row no longer exists — a swept row (or one
    /// that left this client's sync scope) is over as far as any client can
    /// tell, and nothing about it is retryable.
    private static let sessionGoneDetail = "This session is no longer available."
    /// Echo-FIFO bounds (EXP-78): a mid-turn steered message can take a while
    /// to hit the transcript, but an unmatched echo must not swallow an
    /// identical message sent much later.
    private static let echoCap = 8
    private static let echoTTLSeconds: Double = 300

    init(
        accountId: String,
        session: SessionEntity,
        currentUserId: String?,
        sessionsApi: SessionsApi,
        attachmentsApi: AttachmentsApi,
        issuesApi: IssuesApi,
        db: DatabaseManager
    ) {
        self.accountId = accountId
        self.sessionId = session.id
        self.currentUserId = currentUserId
        self.sessionsApi = sessionsApi
        self.attachmentsApi = attachmentsApi
        self.issuesApi = issuesApi
        self.db = db
        self.session = session
        self.root = session.isRoot ? session : nil
    }

    /// Bind the synced session row and auto-connect once when presented;
    /// drops after that auto-reconnect (EXP-243).
    func start() {
        startObservingSession()
        startObservingHostDevice()
        startObservingMentionMembers()
        configureDraftEditor()
        startActivityClock()
        openViewStats()
        if phase == .idle { connect() }
    }

    /// Dial (or re-dial) the session's device over eNet.
    ///
    /// - Parameter force: dial even from `.connecting` (EXP-625). That phase
    ///   used to be an unconditional early-return, which is exactly what made a
    ///   wedged dial unrecoverable: a dial that never returns leaves the model
    ///   at `.connecting` with nothing
    ///   in flight and every revival path bouncing off the guard. `dial()`
    ///   bumps `dialGeneration`, so a forced connect makes the wedged dial's
    ///   callbacks inert. `.live` stays guarded: a live socket needs no dial.
    func connect(force: Bool = false) {
        if !force, phase == .connecting { return }
        guard phase != .live else { return }
        logger.info("connect force=\(force) phase=\(Self.describe(self.phase), privacy: .public)")
        stopped = false
        retryTask?.cancel()
        retryTask = nil
        reconnectAttempts = 0
        resetDialState()
        phase = .connecting
        Task { await dial() }
    }

    /// Every wake signal lands here (EXP-625): foregrounding, opening the
    /// screen, and the network coming back. The decision is the pure rule in
    /// ExpCore (SteerReconnectPolicy.revive) and it gates on whether a dial is
    /// ACTUALLY alive, not on the phase. Phase-gating is what stranded viewers
    /// on "Connecting…" after a background: the wedged states matched no branch
    /// of the old `reconnectNow()`, so nothing left in the process could revive
    /// them.
    func kick(_ reason: String) {
        // FEED-26: a backgrounded process runs no timers, so the quiet clock
        // comes back as stale as everything else — re-stamp it on the way in
        // instead of showing a minute-old count until the next tick.
        activityNow = Date()
        // VAPP-169: the device-offline `ended` is final to the revival rule
        // but not to the offline redial, which a wakeup always fires.
        if offlineRedialPhase == .offline {
            offlineRedial(.wakeup)
            return
        }
        let stale = phase == .live
            && (lastFrameAt.map { Date().timeIntervalSince($0) > Self.liveStaleSeconds } ?? true)
        let decision = SteerReconnectPolicy.revive(
            phase: phaseKind,
            dialInFlight: dialInFlight,
            retryArmed: retryArmed,
            finished: stopped || isOver,
            socketStale: stale
        )
        let phaseName = Self.describe(phase)
        let decisionName = String(describing: decision)
        logger.info(
            "kick \(reason, privacy: .public) phase=\(phaseName, privacy: .public) dialActive=\(self.dialActive) decision=\(decisionName, privacy: .public)"
        )
        switch decision {
        case .dial:
            connect(force: true)
        case .wakeRetry:
            // A backoff or starting-retry wait is ARMED (never a dial in
            // flight — the policy rules that out) but the user is asking now.
            // Dial immediately WITHOUT touching the phase, so `.starting` and
            // the "Reconnecting…" banner hold steady across the redial.
            retryTask?.cancel()
            retryTask = nil
            resetDialState()
            Task { await dial() }
        case .redialSilently:
            redialNow()
        case .nothing:
            // Live and fresh: the stale watch (`armStaleWatch`) re-joins and
            // then drops a carrier that went quiet. #EXP-1528: on the relay,
            // eNet's background redial stops waiting out its backoff.
            carriers?.wakeup()
        }
    }

    /// A retry wait is parked, waiting to start a dial. Only THIS may be cut
    /// short by a wake signal — an in-flight dial is left to finish, or the
    /// kick would open a second socket beside the first one (EXP-625).
    private var retryArmed: Bool {
        retryTask.map { !$0.isCancelled } ?? false
    }

    /// A dial is in flight, or a retry wait is armed to start one. The one
    /// thing `kick` gates on (EXP-625).
    private var dialActive: Bool {
        dialInFlight || retryArmed
    }

    /// Revive after a shutdown(): re-arm the session observation and redial
    /// (the device replays the full activity log on join, rebuilding the feed).
    /// Since EXP-621 the socket outlives the screen — SteerSessionStore only
    /// shuts a model down when it retires it — so this is a no-op on the
    /// normal navigate-away-and-back path, kept for the paths that do stop.
    func resume() {
        guard stopped else { return }
        startObservingSession()
        startObservingHostDevice()
        startActivityClock()
        openViewStats()
        connect()
    }

    /// Tear everything down. Called by SteerSessionStore when it retires this
    /// session (over, evicted, or the account went away) — NOT when the screen
    /// merely goes off-view (EXP-621).
    func shutdown() {
        logger.info("shutdown")
        endViewStats()
        stopped = true
        retryTask?.cancel()
        retryTask = nil
        dialInFlight = false
        lastFrameAt = nil
        dropAt = nil
        failedRedialsSinceDrop = 0
        cancelAnswerExpiries()
        clearLiveWork()
        sessionObservationTask?.cancel()
        sessionObservationTask = nil
        deviceObservationTask?.cancel()
        deviceObservationTask = nil
        deviceLivenessTask?.cancel()
        deviceLivenessTask = nil
        deviceFreshnessTask?.cancel()
        deviceFreshnessTask = nil
        activityClockTask?.cancel()
        activityClockTask = nil
        mentionObservationTask?.cancel()
        mentionObservationTask = nil
        connected = false
        pendingFrames = []
        discardStaging()
        // Reset to the pre-connection state so a later resume() can redial: the
        // link is gone, so leaving phase at .live/.connecting would make
        // connect()'s guard early-return and the reopened view would show a
        // stale "live" status over a dead link (EXP-221). The normal
        // link-drop path sets .closed itself and never routes through here.
        phase = .idle
        closeFeed()
    }

    // MARK: - Messages (EXP-1295: the database is the bus)

    /// Send one message to the session: a `messages` row through
    /// `sessions.send({sessionId})`. An idle session resumes on it; sent
    /// mid-turn, the device holds it until the turn ends (the steer feed's
    /// `queue` slot shows it meanwhile).
    ///
    /// Returns whether the message actually went out (EXP-621): a caller
    /// clears the draft only then.
    ///
    /// EXP-820: the text is ALWAYS a message. A pending card's free answer is
    /// its own inline field (`sendAnswer` with `text`, or `answerThenSend` for
    /// a plan's feedback).
    /// #EXP-1715: the viewer's Assistant root's Teams pick, carried by every
    /// send as `scopeTeamIds` (set by the host view; nil = none).
    var sendScope: TeamsScope?
    /// #FEED-204 / #EXP-1732: an app editor's ended root revived on another
    /// device: the next send is `sessions.send({appId, deviceId})` (set by the
    /// host view; nil = a plain message).
    var sendAppMove: ChatComposerDraft.AppMove?

    @discardableResult
    func sendMessage(_ text: String, attachmentIds: [String]? = nil) async -> Bool {
        guard !text.isEmpty || attachmentIds?.isEmpty == false, mayPost else { return false }
        sendError = nil
        // One send at a time: the composer dims while this is out (an
        // attachment send already holds the flag through its uploads).
        let holdsFlag = !steerSending
        if holdsFlag { steerSending = true }
        defer { if holdsFlag { steerSending = false } }
        do {
            if let move = sendAppMove {
                _ = try await sessionsApi.send(
                    accountId: accountId,
                    .app(
                        appId: move.appId, deviceId: move.deviceId, text: text, attachmentIds: attachmentIds,
                        // #EXP-1729: with the editor composer's Account and Model picks.
                        cli: move.cli, account: move.account, model: move.model
                    )
                )
            } else {
                try await sessionsApi.send(
                    accountId: accountId,
                    .message(sessionId: sessionId, text: text, attachmentIds: attachmentIds, scope: sendScope)
                )
            }
        } catch {
            sendError = error.trpcUserMessage
            return false
        }
        // EXP-861: sent mid-turn or mid-compaction, the DEVICE holds it — no
        // local echo then: the next `queue` frame shows it in the strip, and
        // the real `user_message` row lands when it is delivered. The raw
        // slot values decide, not the full working predicate (×4 parity).
        if turn.state == .started || compacting != nil { return true }
        // Local echo (EXP-78): show the sent message immediately; its
        // transcript-derived `user_message` event is deduped via the FIFO.
        let rowId = takeEventId()
        recentEchoes.append(AgentEcho(
            text: text.trimmingCharacters(in: .whitespacesAndNewlines), at: Date(), rowId: rowId
        ))
        if recentEchoes.count > Self.echoCap {
            recentEchoes.removeFirst(recentEchoes.count - Self.echoCap)
        }
        append(.userMessage(id: rowId, text: text, at: Int(Date().timeIntervalSince1970 * 1000)))
        // EXP-656: sent while a replay stages — the replay predates it, so the
        // commit re-appends it unless the replay happened to carry it back.
        if isStaging { stagedLocalEchoes.append(text) }
        return true
    }

    /// Answer a protocol-v2 question card (EXP-249): ONE semantic `answer`
    /// frame carrying every chosen key — the desktop owns the keystroke
    /// mapping and confirms the injection with `answer_ack`. The card locks
    /// the moment the frame goes out, so a double tap can never answer twice.
    ///
    /// EXP-820 `reanswer`: the stepper's "go back" — an EARLIER step of a
    /// still-open ask is answered again past its lock. The engine re-records
    /// the step and re-acks (plus a fresh `question_resolved` for it) without
    /// moving the stepper; `markSent` keeps the step's `acked` standing.
    func sendAnswer(
        questionId: String, askId: String?, keys: [String], text: String? = nil,
        labels: [String] = [], reanswer: Bool = false
    ) {
        guard !questionId.isEmpty, !keys.isEmpty, connected else { return }
        guard reanswer || !answerTracker.isLocked(questionId) else { return }
        var frame: [String: Any] = ["t": "answer", "questionId": questionId, "keys": keys]
        if let askId, !askId.isEmpty { frame["askId"] = askId }
        // EXP-513: the typed reply for a freeText option.
        if let text, !text.isEmpty { frame["text"] = text }
        if let data = try? JSONSerialization.data(withJSONObject: frame),
           let json = String(data: data, encoding: .utf8) {
            sendText(json)
        }
        lockAnswer(questionId, labels: labels)
    }

    /// EXP-820: a plan card's inline feedback — deny the plan on its reject
    /// row ("No, keep planning" — the engine's deny interrupts the turn), then
    /// send `text` as the NEXT ordinary message, which is what that option's
    /// own description promises. Nothing goes out unless the answer can (a
    /// locked card or a dead socket), so the text never lands as a stray
    /// message on an unanswered plan.
    func answerThenSend(
        questionId: String, askId: String?, keys: [String], labels: [String], text: String,
        reanswer: Bool = false
    ) {
        guard connected, reanswer || !answerTracker.isLocked(questionId) else { return }
        sendAnswer(
            questionId: questionId, askId: askId, keys: keys, labels: labels, reanswer: reanswer
        )
        let message = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !message.isEmpty else { return }
        Task { await sendMessage(message) }
    }

    /// EXP-746: switch to one of the modes `config_state.modes[]` advertised.
    ///
    /// Fire-and-forget — the publisher re-emits `config_state` once it
    /// applied, and THAT repaint is the confirmation, so there is no lock and
    /// no expiry task (unlike `sendAnswer`). An agent that refuses simply
    /// re-emits the old mode and the chip snaps back.
    ///
    /// Gated on `canSteer`, not merely `connected`, so the chip dims as
    /// honestly as the send button on a paused or ended run.
    ///
    /// EXP-772 retired the `set_config` sender with the option chips: model
    /// and effort are launch decisions now, and the engine publishes an empty
    /// `config_state.options` to say so.
    func sendMode(id: String) {
        guard !id.isEmpty, canSteer else { return }
        send(frame: ["t": "set_mode", "id": id])
    }

    /// EXP-790/EXP-1295: stop the agent's current turn (`sessions.stop`) —
    /// the header's Stop and the composer's Stop glyph while the agent works
    /// and the field is empty. The turn's own `idle` edge is the
    /// confirmation; a refusal lands in `stopError`.
    ///
    /// EXP-873: a Stop drops every message the agent has not read yet (the
    /// queue strip), so their text goes back into the composer, in order,
    /// instead of vanishing; the device's empty `queue` frame confirms. A
    /// draft in progress keeps its place, the queued text lands after a
    /// blank line.
    func stop() {
        guard canStop else { return }
        stopError = nil
        restoreQueueToDraft()
        let accountId = accountId
        let sessionId = sessionId
        Task { [weak self] in
            do {
                let result = try await self?.sessionsApi.stop(accountId: accountId, sessionId: sessionId)
                if result?.delivered == false { self?.stopNotDeliveredAt = Date() }
            } catch {
                self?.stopError = error.trpcUserMessage
            }
        }
    }

    /// EXP-873: hand the strip's text back to the composer and clear it.
    private func restoreQueueToDraft() {
        guard !queued.isEmpty else { return }
        let held = queued.map(\.text).joined(separator: "\n")
        queued = []
        draftText = trimmedDraft.isEmpty ? held : draftText + "\n\n" + held
    }

    private func send(frame: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: frame),
              let json = String(data: data, encoding: .utf8) else { return }
        sendText(json)
    }

    /// EXP-861/EXP-1295: revoke a queued message (`sessions.unqueue` — a
    /// queued line IS its `messages` row). Fire-and-forget — the device's
    /// next `queue` frame is the confirmation — but the line leaves the strip
    /// at once, and its text goes back into an EMPTY draft so it can be
    /// edited and re-sent; a draft in progress is never overwritten.
    func unqueue(_ id: String) {
        guard let index = queued.firstIndex(where: { $0.id == id }) else { return }
        let message = queued.remove(at: index)
        if trimmedDraft.isEmpty { draftText = message.text }
        let accountId = accountId
        Task { [sessionsApi] in
            try? await sessionsApi.unqueue(accountId: accountId, messageId: id)
        }
    }

    /// Lock a card and arm ITS OWN expiry that frees it again (flagged
    /// `failed`, so the card shows a retry hint) if neither an `answer_ack`
    /// nor a `question_resolved` ever lands. Per-card timers (EXP-334): a
    /// shared one was cancelled by each newer lock and then expired every
    /// pending card at once.
    private func lockAnswer(_ lockKey: String, labels: [String] = []) {
        answerTracker.markSent(lockKey, labels: labels)
        answerExpiryTasks[lockKey]?.cancel()
        answerExpiryTasks[lockKey] = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.answerLockSeconds))
            guard let self, !Task.isCancelled else { return }
            self.answerExpiryTasks[lockKey] = nil
            self.answerTracker.expire(lockKey)
        }
    }

    /// Cancel every armed expiry timer (teardown / feed reset).
    private func cancelAnswerExpiries() {
        for task in answerExpiryTasks.values { task.cancel() }
        answerExpiryTasks = [:]
    }

    // MARK: - Synced session row

    private func startObservingSession() {
        guard sessionObservationTask == nil else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let id = sessionId
        let observation = ValueObservation.tracking { db -> (SessionEntity?, SessionEntity?, String?, String?, Bool) in
            guard let row = try SessionQueries.session(db: db, id: id) else { return (nil, nil, nil, nil, false) }
            let root = row.isRoot ? row : try SessionQueries.session(db: db, id: row.rootId)
            let title = try SessionTree.sessionTitle(db: db, session: row)
            let chatTitle = try root.map { try SessionTree.sessionTitle(db: db, session: $0) }
            let hasSubSessions = try row.isRoot
                && SessionQueries.tree(db: db, rootId: row.id).contains { $0.id != row.id }
            return (row, root, title, chatTitle, hasSubSessions)
        }
        sessionObservationTask = Task { [weak self] in
            // The GRDB stream is one-shot: any error (a busy database, a
            // reclaimed pool) ends the async sequence for good. Re-subscribe
            // instead of dying silently — a dead observation freezes
            // `session` at its last value, so a row that flips to `ended`
            // server-side never lands and "Working…" stays up forever
            // (EXP-410; Android's Room flow re-subscribes on its own).
            while !Task.isCancelled {
                do {
                    for try await (row, root, title, chatTitle, hasSubSessions) in observation.values(in: pool) {
                        guard let self else { return }
                        // A nil row is published (not swallowed): the row is
                        // gone, which `sessionEnded` reads as ended. Holding
                        // the last known copy instead left the retry loops
                        // waiting for an `ended` status a deleted row can
                        // never report.
                        // #EXP-1623: only what moved is written; an equal
                        // write still re-renders every view that reads it.
                        if row != self.session { self.session = row }
                        if root != self.root { self.root = root }
                        if hasSubSessions != self.hasSubSessions { self.hasSubSessions = hasSubSessions }
                        if let title, title != self.title { self.title = title }
                        if let chatTitle, chatTitle != self.chatTitle { self.chatTitle = chatTitle }
                        // EXP-724: the run is over (or gone) — no `ended`
                        // compaction frame will ever arrive for it.
                        if self.sessionEnded { self.clearLiveWork() }
                        self.liftRowEndedIfRunBack()
                        self.rebuildHostDevice()
                    }
                    // The sequence only finishes cleanly on cancellation.
                    return
                } catch is CancellationError {
                    return
                } catch {
                    try? await Task.sleep(for: .seconds(1))
                }
            }
        }
    }

    /// EXP-549/550: watch the synced `devices` rows so the header names the
    /// machine by its CURRENT label and flips to "paused" when it goes quiet.
    /// A 30s tick re-derives liveness on its own — the heartbeat stopping is
    /// the absence of a write, which no ValueObservation can report (the
    /// AgentsViewModel liveness clock, same cadence against the 90s window).
    private func startObservingHostDevice() {
        guard deviceObservationTask == nil else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try DeviceEntity.fetchAll(db)
        }
        deviceObservationTask = Task { [weak self] in
            // Same re-subscribe loop as the session observation: the GRDB
            // stream is one-shot, and a dead one would freeze the machine at
            // its last known state forever.
            while !Task.isCancelled {
                do {
                    for try await rows in observation.values(in: pool) {
                        guard let self else { return }
                        // #EXP-1625: every devices-table write (a heartbeat)
                        // re-emits; an equal list must not invalidate readers.
                        if rows != self.deviceRows { self.deviceRows = rows }
                        self.rebuildHostDevice()
                    }
                    return
                } catch is CancellationError {
                    return
                } catch {
                    try? await Task.sleep(for: .seconds(1))
                }
            }
        }
        deviceLivenessTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                guard let self, !Task.isCancelled else { return }
                self.rebuildHostDevice()
            }
        }
        // EXP-656: and the third input — our own devices cursor advancing,
        // which is neither a row write nor a clock tick.
        deviceFreshnessTask = Task { [weak self] in
            for await polledAccountId in SyncFreshness.shared.updates() {
                guard let self, !Task.isCancelled else { return }
                guard polledAccountId == self.accountId else { continue }
                self.rebuildHostDevice()
            }
        }
    }

    /// FEED-26: re-stamp `activityNow` every 30s so a quiet run's caption
    /// counts up on its own. Same cadence as the host-liveness clock, and for
    /// the same reason: nothing writes while a machine (or an agent) goes
    /// silent, so no observation can report it.
    private func startActivityClock() {
        guard activityClockTask == nil else { return }
        activityClockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                guard let self, !Task.isCancelled else { return }
                self.activityNow = Date()
            }
        }
    }

    /// FEED-26: the feed moved (or the run just came live) — restart the
    /// quiet clock.
    private func markActivity() {
        let now = Date()
        let wasStale = AgentFeed.staleActivityMinutes(since: lastActivityAt, now: activityNow) != nil
        lastActivityAt = now
        // #EXP-1623: `activityNow` is OBSERVED (the header, the Dock's
        // banners), and this runs on every feed write: only a run that read
        // quiet re-stamps it at once, the 30s clock does the rest.
        if wasStale { activityNow = now }
    }

    private func rebuildHostDevice() {
        guard let session else { return }
        let now = Date()
        let hostRow = Self.hostRow(session: session, devices: deviceRows)
        // EXP-656: a `last_seen_at` we haven't refreshed since before the
        // suspension is ignorance, not evidence — presence resolves to unknown
        // (never "Paused") until the devices shape has polled inside its
        // contract window.
        let devicesFresh = DeviceFreshness.isTrustworthy(
            devicesPolledAt: SyncFreshness.shared.devicesPolledAt(accountId: accountId),
            now: now
        )
        let wasOnline = hostDevice.online
        // #EXP-1623: every device's heartbeat lands here (the devices shape
        // is one table); each slot below is written only when it moved, so a
        // teammate's machine checking in re-renders nothing of this session.
        let nextHost = SessionHostDevice(
            label: hostRow.flatMap { $0.label.isEmpty ? nil : $0.label },
            online: hostRow.map { row in
                DeviceLiveness.isOnline(lastSeenAt: row.lastSeenAt, now: now)
                    ? true : (devicesFresh ? false : nil)
            } ?? nil
        )
        let hostMoved = nextHost != hostDevice || hostRow != hostSteerRow
        if nextHost != hostDevice { hostDevice = nextHost }
        // VAPP-169: a presence flip is an offline-redial trigger (online
        // redials the offline copy and cuts a `starting` backoff short).
        if hostDevice.online != wasOnline {
            if hostDevice.online == true {
                offlineRedial(.presenceOnline)
            } else if hostDevice.online == false {
                offlineRedial(.presenceOffline)
            }
        }
        let nextUsage = AgentUsagePresentation.sessionUsage(
            status: session.status,
            cli: session.cli,
            deviceId: session.deviceId,
            userId: session.userId,
            devices: deviceRows,
            now: now
        )
        if nextUsage != agentUsage { agentUsage = nextUsage }
        let hostAccounts = AgentUsagePresentation.parseAccounts(hostRow?.agentAccounts)
        let nextAccount = session.cli.flatMap { hostAccounts?[$0] }
        if nextAccount != agentAccount { agentAccount = nextAccount }
        if hostMoved {
            hostSteerRow = hostRow
            hostSteerDevice = hostRow.map {
                SteerDevice(entity: $0, now: now, currentUserId: currentUserId)
            }
        }
        // EXP-909: which of the host's logins this session spends, and that
        // login's numbers — off the same ×4 rows the Devices page draws.
        let logins = hostSteerDevice.map(AgentAccountsRows.deviceLoginRows)?
            .filter { $0.agent == session.cli } ?? []
        let reportedEmail = session.cli.flatMap { hostAccounts?[$0]?.email }
        let nextRunAccount = logins.first { $0.profileId == session.account }
            ?? reportedEmail.flatMap { email in logins.first { $0.email == email } }
            ?? logins.first { $0.active }
        if nextRunAccount != runAccount { runAccount = nextRunAccount }
        let reportedUsage = session.cli.flatMap {
            AgentUsagePresentation.parseMap(hostRow?.agentUsage)?[$0]
        }
        // The machine only ever puts its ACTIVE login's numbers in the
        // top-level map, so that fallback is legitimate for the active profile
        // and for an unknown account — and a lie for any other profile.
        let nextRunUsage = runAccount?.usage ?? (runAccount?.active != false ? reportedUsage : nil)
        if nextRunUsage != runUsage { runUsage = nextRunUsage }
    }

    /// #EXP-1623: the host row `hostSteerDevice` was last built from.
    @ObservationIgnored private var hostSteerRow: DeviceEntity?

    /// The session's host `devices` row: the stamped `device_id`, preferring
    /// the session starter's own row.
    private static func hostRow(
        session: SessionEntity,
        devices: [DeviceEntity]
    ) -> DeviceEntity? {
        let byId = devices.filter { $0.deviceId == session.deviceId }
        return byId.first { $0.userId == session.userId } ?? byId.first
    }

    /// EXP-909: ONE `agent_usage_refresh` as the Usage overlay opens — never a
    /// polling loop, because the heartbeat delivers the answer within 30 s.
    ///
    /// Only on MY machine, only while it is listening, and only past the
    /// device's own 429 floor (`refreshAllowedAt`) — the same three gates the
    /// Devices page's refresh round applies. It fails quietly: a
    /// command still queued from a previous open is a CONFLICT, and the next
    /// open simply looks again.
    func requestUsageRefresh() {
        guard let devicesApi, let device = hostSteerDevice, let agent = session?.cli else { return }
        guard device.isMine, device.isOnline,
              AgentAccountsRows.refreshAllowedAt(runUsage, now: Date()) == nil
        else { return }
        let accountId = accountId
        let profileId = runAccount?.profileId
        Task {
            _ = try? await devicesApi.createCommand(
                accountId: accountId,
                deviceId: device.deviceId,
                kind: "agent_usage_refresh",
                agent: agent,
                profileId: profileId
            )
        }
    }

    // MARK: - Composer autocomplete (EXP-802)

    /// Point the draft editor's three typeaheads at this run's team.
    ///
    /// `#` refs resolve through the `.team` scope the feed's chips already use
    /// (EXP-760) — a session has no single issue or board to derive a team
    /// from. `:` emoji need
    /// nothing scoped: the catalog is process-global. The `@` vocabulary is
    /// membership, so it arrives on the observation below instead.
    private func configureDraftEditor() {
        guard draftEditor.issueRefSearch == nil, let teamId = session?.teamId else { return }
        let scope = IssueRefLookup.Scope.team(id: teamId)
        let database = db
        let account = accountId
        draftEditor.issueRefResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: scope, db: database, accountId: account)?
                .issueId
        }
        draftEditor.issueRefTitleResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: scope, db: database, accountId: account)?
                .title
        }
        draftEditor.issueRefStatusResolver = { identifier in
            IssueRefChipCache.statusInfo(
                identifier, scope: scope, db: database, accountId: account)
        }
        // EXP-892: the locally ranked rows render instantly; a debounced
        // `issues.search` splices the server's full-text hits (comment bodies
        // included) in behind them — the same `#` menu every other editor has.
        let augmentor = IssueRefAugmentor(
            scope: scope,
            db: database,
            accountId: account,
            issuesApi: issuesApi
        )
        issueRefAugmentor = augmentor
        augmentor.attach(to: draftEditor)
        // EXP-551: decode the bundled dataset off-main once, exactly as the
        // markdown editor does on mount.
        EmojiCatalog.shared.preload()
        draftEditor.emojiSearch = { query in
            EmojiCatalog.shared.search(query, limit: EmojiCatalog.typeaheadLimit)
        }
        draftEditor.onEmojiInserted = { record in
            EmojiPreferences().recordRecent(record.unicode)
        }
    }

    /// The @-mention vocabulary, on the same two tables `IssueDetailViewModel`
    /// watches. Both are synced whole and small, so the team filter runs IN
    /// MEMORY (`rebuildMentionMembers`) rather than in SQL — the alternative is
    /// re-querying on a predicate whose input, the session row, arrives on a
    /// different observation.
    private func startObservingMentionMembers() {
        guard mentionObservationTask == nil else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> ([UserEntity], [TeamMemberEntity]) in
            (try UserEntity.fetchAll(db), try TeamMemberEntity.fetchAll(db))
        }
        mentionObservationTask = Task { [weak self] in
            // Same one-shot re-subscribe loop as the session observation
            // (EXP-410): a dead stream would freeze the vocabulary at whatever
            // had synced when the screen opened.
            while !Task.isCancelled {
                do {
                    for try await (users, members) in observation.values(in: pool) {
                        guard let self else { return }
                        self.mentionUserRows = users
                        self.mentionMemberRows = members
                        self.rebuildMentionMembers()
                    }
                    return
                } catch is CancellationError {
                    return
                } catch {
                    try? await Task.sleep(for: .seconds(1))
                }
            }
        }
    }

    /// EXP-487's rule, session-scoped: the user rows stay account-wide (an
    /// ex-member still renders elsewhere), and membership is what an `@` may
    /// name.
    private func rebuildMentionMembers() {
        guard let teamId = session?.teamId else { return }
        let memberIds = Set(mentionMemberRows.filter { $0.teamId == teamId }.map(\.userId))
        draftEditor.mentionMembers = mentionUserRows
            .filter { memberIds.contains($0.id) }
            .map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
    }

    // MARK: - Connect lifecycle

    private func resetDialState() {
        sawEnd = false
        retryStarting = false
        endDetail = nil
    }

    private func dial() async {
        // Whatever the last dial left open is superseded here and now: closing
        // it before bumping the generation means the device drops that viewer
        // instead of keeping a joined socket nobody owns, and the close lands
        // on a generation this model no longer answers to.
        disconnectSocket()
        dialGeneration += 1
        let generation = dialGeneration
        // EXP-625: the retry slot has done its job. A finished-but-uncancelled
        // task left sitting there reads as an armed wait forever, and `kick`
        // would keep answering `wakeRetry` to a model with nothing pending.
        retryTask = nil
        // EXP-625: from here until this dial produces an outcome, a wake signal
        // leaves it alone. Every exit below clears the flag, or a wedged dial
        // would look alive forever and re-strand the model.
        dialInFlight = true
        dialStartedAt = Date()
        lastFrameAt = nil
        // #EXP-1528: eNet first, the sealed relay after `stallMs` / no feed /
        // a drop; the first carrier to deliver ends this dial.
        let feed = SteerFeedCarriers(
            accountId: accountId,
            sessionId: sessionId,
            enet: steerEnet,
            tunnel: tunnelDialer(),
            onSignal: { [weak self] signal in self?.onCarrier(signal, generation: generation) }
        )
        carriers = feed
        feed.start()
    }

    /// #EXP-1528: one signal of the current feed. Carriers stay invisible:
    /// a swap is just the new carrier's replay (staged like any re-join).
    private func onCarrier(_ signal: SteerFeedCarriers.Signal, generation: Int) {
        guard !stopped, generation == dialGeneration else { return }
        switch signal {
        case let .active(carrier):
            if let settled = viewStats?.active(carrier, enetPath: carriers?.enetPath) { recordStats([settled]) }
            if !connected { attachFeed(generation: generation) }
        case let .frames(frames):
            for frame in frames { enqueue(frame, generation: generation) }
        case .fallback:
            viewStats?.fallback()
        case let .failure(errorClass):
            viewStats?.failure(errorClass)
        case let .noFeed(reason):
            carriers = nil
            dialInFlight = false
            logger.info("no feed on either carrier (eNet \(reason?.rawValue ?? "-", privacy: .public))")
            if dropAt != nil, !sessionEnded {
                // EXP-1487: a redial of a dropped feed that failed — silent
                // until the lost-banner rule says otherwise.
                feedDown()
            } else {
                dropAt = nil
                applyNoFeed()
            }
        case .closed:
            disconnectSocket()
            onSocketClosed(generation: generation)
        }
    }

    /// #EXP-1528: the relay leg of this session, when this build has one:
    /// the root's seal key + the secret this install holds, then the ticket.
    private func tunnelDialer() -> SteerTunnelDialing? {
        guard let steerTunnel else { return nil }
        let rootId = session?.rootId ?? root?.id ?? sessionId
        var key = root.flatMap { $0.id == rootId ? $0.sealKey : nil }
        if key == nil, let pool = try? db.pool(forAccountId: accountId) {
            key = (try? pool.read { try SessionEntity.fetchOne($0, key: rootId) })??.sealKey
        }
        let opener = SealedOpener.shared
        let rootKey = key
        return SteerTunnelDialer(accountId: accountId, api: steerTunnel, sealer: {
            SteerTreeSealer(crypto: opener.crypto, ring: opener.ring, rootId: rootId, key: rootKey)
        })
    }

    // MARK: - Connection analytics (#EXP-1528)

    private func openViewStats() {
        guard viewStats == nil else { return }
        viewStats = SteerViewStats(version: AppConstants.appVersion)
        settleTask?.cancel()
        settleTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(SteerConnStats.settleAfterMs))
            guard let self, !Task.isCancelled else { return }
            if let settled = self.viewStats?.settleTimeout(background: Self.appInBackground) { self.recordStats([settled]) }
        }
    }

    private func endViewStats() {
        settleTask?.cancel()
        settleTask = nil
        guard var stats = viewStats else { return }
        viewStats = nil
        recordStats(stats.end(background: Self.appInBackground))
    }

    /// #EXP-1678: a view that fails while the app is away says `background`.
    private static var appInBackground: Bool {
        UIApplication.shared.applicationState == .background
    }

    private func recordStats(_ events: [SteerConnEvent]) {
        connStats?.record(accountId: accountId, events)
    }

    /// VAPP-43 NO FEED (fixtures/steer-enet.json): a row still alive is
    /// `starting` and redials on its cadence; an ended row is `ended` with
    /// the device-offline copy (a vanished row keeps its own "no longer
    /// available" caption).
    private func applyNoFeed() {
        switch SteerNoFeedState.decide(sessionOver: sessionEnded) {
        case .starting:
            endDetail = Self.startingDetail
            phase = .starting
            // #EXP-1703 (steer-enet.json A DEVICE THAT IS AWAY): a live row
            // on an offline host redials on the device-offline cadence (and
            // at once on the presence flip online), not the starting backoff.
            if hostDeviceOffline {
                scheduleAwayRedial()
            } else {
                scheduleStartingRetry()
            }
        case .endedDeviceOffline:
            retryTask = nil
            if session == nil {
                phase = .ended(detail: Self.sessionGoneDetail)
            } else {
                history = .deviceOffline
                phase = .ended(detail: nil)
                scheduleOfflineRedial()
            }
        }
    }

    /// VAPP-169: the row the offline-redial rule sees — nil for every phase
    /// that rule says nothing about (connecting, live, a reconnecting close).
    private var offlineRedialPhase: SteerOfflineRedial.Phase? {
        switch phase {
        case .ended: return history == .deviceOffline && session != nil ? .offline : .ended
        case .starting: return .starting
        default: return nil
        }
    }

    /// VAPP-169 OFFLINE REDIAL: dial again WITHOUT touching the phase, so the
    /// device-offline copy (or `starting`) holds across the attempt. NO FEED
    /// lands back in `applyNoFeed` (the same copy, the timer re-armed); a feed
    /// is the transcript (`markLive`).
    private func offlineRedial(_ trigger: SteerOfflineRedial.Trigger) {
        guard !stopped, !dialInFlight, let redialPhase = offlineRedialPhase,
              SteerOfflineRedial.decide(phase: redialPhase, trigger: trigger) else { return }
        logger.info("offline redial \(trigger.rawValue, privacy: .public) phase=\(redialPhase.rawValue, privacy: .public)")
        retryTask?.cancel()
        retryTask = nil
        resetDialState()
        Task { await dial() }
    }

    /// VAPP-169: while the device-offline copy shows, redial every `staleMs`
    /// (presence lags, or is off).
    private func scheduleOfflineRedial() {
        retryTask?.cancel()
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(SteerEnet.staleMs))
            guard let self, !self.stopped, !Task.isCancelled else { return }
            self.retryTask = nil
            self.offlineRedial(.staleTimer)
        }
    }

    /// #EXP-1703: a live row on an away host redials every `staleMs`, the
    /// device-offline cadence (web `scheduleAwayRedial`).
    private func scheduleAwayRedial() {
        retryTask?.cancel()
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(SteerEnet.staleMs))
            guard let self, !self.stopped, !Task.isCancelled else { return }
            self.retryTask = nil
            guard !self.dialInFlight, SteerOfflineRedial.decide(phase: .offline, trigger: .staleTimer) else { return }
            self.resetDialState()
            await self.dial()
        }
    }

    /// A frame the device only sends AFTER a successful join (`activity_reset`
    /// goes out immediately on join; a bad one answers `no_such_session`
    /// instead) — the single proof the feed is really live.
    /// Mirrors the Android `FrameResult.live` handling.
    private func markLive() {
        // EXP-773: a device answered, so whatever arrives next IS the
        // transcript. #EXP-1623: written only when set; this runs on every
        // frame, and the transcript reads it (a write per tick re-rendered it).
        if history != nil { history = nil }
        // EXP-796/VAPP-43: an ENDED run's transcript comes down a joined link
        // too — the device's stored replay, closed by `bye {history}` — and
        // that link is not a live run. The synced row is the truth: the phase
        // goes straight to `ended` (the header, the composer and every
        // steering gate already read the row), and paging keeps riding the
        // link until it really closes (`connected`, not the phase, gates
        // `canLoadEarlier`).
        if sessionEnded {
            if case .ended = phase { return }
            phase = .ended(detail: endDetail)
            return
        }
        guard phase != .live else { return }
        phase = .live
        reconnectAttempts = 0
    }

    /// VAPP-173: a feed that answered while the row still read ended landed
    /// on `.ended` from the ROW (`markLive`); the run came back
    /// (fixtures/steer-enet.json OFFLINE REDIAL: "a run that came back =
    /// live"), so the row flipping running under that open feed lifts it.
    /// Keepalives alone (an idle run's joined viewer) never re-evaluate.
    /// Mirrors Android's `SteerConnection` row collector.
    private func liftRowEndedIfRunBack() {
        guard !sessionEnded, connected, !sawEnd, history == nil else { return }
        guard case .ended = phase else { return }
        phase = .live
        reconnectAttempts = 0
    }

    /// Queue a frame and arm one flush for the batch (EXP-582). The flush is a
    /// plain main-actor task: everything the link delivered before it gets
    /// to run — the whole burst of a replay — is applied together, and a lone
    /// live frame still lands within the same run-loop turn.
    private func enqueue(_ text: String, generation: Int) {
        guard !stopped, connected, generation == dialGeneration else { return }
        // EXP-625: the first frame IS the join answer, so it ends this dial and
        // disarms the deadline. Every frame restamps `lastFrameAt`, which is
        // what a wake signal reads to tell a live link from a quiet corpse.
        if lastFrameAt == nil {
            let elapsed = dialStartedAt.map { Date().timeIntervalSince($0) } ?? -1
            logger.info("dial answered, first frame in \(elapsed, format: .fixed(precision: 1))s")
            dialInFlight = false
        }
        lastFrameAt = Date()
        pendingFrames.append((text, generation))
        guard !flushScheduled else { return }
        flushScheduled = true
        Task { @MainActor [weak self] in await self?.flush() }
    }

    /// #EXP-1623: a batch of at most this many frames parses inline (a live
    /// frame still lands in the same run-loop turn); a bigger one, a join
    /// replay's burst, parses and decodes OFF the main thread first.
    private static let inlineParseLimit = 16
    /// #EXP-1623: a burst is parsing off main (the staging watcher waits).
    @ObservationIgnored private var parsingBurst = false

    /// Apply everything queued, in arrival order. Frames that queue while a
    /// burst parses off main are the next pass of the same flush.
    private func flush() async {
        defer { flushScheduled = false }
        while !pendingFrames.isEmpty {
            let batch = pendingFrames
            pendingFrames = []
            let texts = batch.map(\.text)
            let parsed: [SteerWireFrame?]
            if texts.count <= Self.inlineParseLimit {
                parsed = texts.map(SteerWireFrame.parse)
            } else {
                parsingBurst = true
                let parseStarted = Date()
                parsed = await Task.detached(priority: .userInitiated) {
                    texts.map(SteerWireFrame.parse)
                }.value
                parsingBurst = false
                // A staging window's quiet and cap clocks do not count the
                // parse: the frames arrived before it began.
                if isStaging {
                    let spent = Date().timeIntervalSince(parseStarted)
                    stagingStartedAt = stagingStartedAt?.addingTimeInterval(spent)
                    lastStagedFrameAt = Date()
                }
            }
            guard !stopped else { return }
            applyingBatch = true
            for (index, frame) in parsed.enumerated() {
                // A frame can tear the link down (`no_such_session`, `bye`);
                // the rest of the batch belonged to that dead feed, and a
                // frame of an earlier dial never reaches this one.
                guard connected else { break }
                guard batch[index].generation == dialGeneration, let frame else { continue }
                onFrame(frame)
            }
            applyingBatch = false
            reproject()
        }
    }

    private func onFrame(_ frame: SteerWireFrame) {
        let t = frame.t
        // EXP-656: the replay frames go through the pure staging rule first —
        // an `activity_reset` no longer wipes anything, it opens a staging
        // window, and the whole replay swaps in as one commit so the reader
        // never sees the feed collapse to empty and get scrolled back down.
        if let staging = SteerReplayStaging.Frame(wire: t) {
            switch SteerReplayStaging.decide(staging: isStaging, frame: staging) {
            case .beginStaging:
                // Also the join's success signal (EXP-312 — the presence frame
                // that used to confirm it is gone).
                markLive()
                beginStaging()
            case .stage:
                markLive()
                stageFrame(frame.activity, seq: frame.seq)
            case .apply:
                markLive()
                currentSeq = frame.seq
                handleActivityEvent(frame.activity)
                currentSeq = nil
            case .commit:
                // `activity_synced` is the device's end-of-replay marker; a
                // `keepalive` (its own beat) proves the burst is over for
                // a republish that carries no marker.
                // EXP-783: the marker also names the SPAN it replayed, so the
                // commit keeps the pages the reader scrolled back to load.
                if staging == .activitySynced {
                    historyTruncated = frame.truncated
                    commitStaging(why: "marker", firstSeq: frame.firstSeq)
                } else {
                    commitStaging(why: "keepalive", firstSeq: nil)
                }
            case .ignore:
                // A keepalive outside a replay (EXP-648) — already counted:
                // `enqueue` stamped `lastFrameAt` before this ran. Never a
                // phase change, never a feed change.
                break
            }
            return
        }
        switch t {
        // EXP-783: one page of OLDER transcript, answering this viewer's
        // `history_page`. PREPENDED, never appended.
        case "history_chunk":
            guard let requestId = frame.requestId,
                  requestId == historyRequest else { return }
            if frame.done { historyRequest = nil }
            prependPage(frame.page, seqs: frame.pageSeqs)
        case "bye":
            // The run ended, or the device's stored replay of an ended run is
            // done (`history`, VAPP-43): either way the device drops this
            // viewer. A protocol word is never a caption (EXP-773).
            sawEnd = true
            endDetail = SteerOutcome.endDetail(frame.outcome)
            disconnectSocket()
            onSocketClosed()
        case "error":
            let code = frame.code ?? "error"
            if code == "no_such_session" {
                // The joined device no longer runs nor holds the session (a
                // re-sent `join` after it lost it): no feed. A live row flips
                // into the auto-retrying starting phase.
                retryStarting = true
                endDetail = Self.startingDetail
                disconnectSocket()
                onSocketClosed()
            } else {
                endDetail = frame.message ?? SteerOutcome.endDetail(code)
            }
        default:
            break // input/kill/legacy presence — not activity-viewer-relevant
        }
    }

    /// Drop everything the room's activity log owns. Local echoes go too: the
    /// replay carries its own copy of every sent message. Only `commitStaging`
    /// calls this, and only inside its one batch — a bare reset is what used to
    /// blank the screen mid-read (EXP-656).
    private func resetFeed() {
        feed = []
        feedBytes = 0
        openThoughts = 0
        seqById = [:]
        testResults = [:]
        // The window anchor survives the swap on purpose (EXP-795, web/Android
        // parity): the replayed prefix keeps its ids, so a reader scrolled up
        // stays where they were instead of being yanked to the newest rows on
        // every reconnect.
        latestDiff = nil
        recentEchoes = []
        answerTracker.reset()
        cancelAnswerExpiries()
        clearLiveWork()
        // EXP-746: the config/usage slots go too (EXP-848: the turn slot with
        // them, via `clearLiveWork` above). `resetFeed` only ever runs inside
        // `commitStaging`, which re-applies every staged frame at once, and the
        // device's join replay always carries the current
        // `config_state`/`usage`/`turn` after the log (latest-wins kinds) — so
        // the chips repaint inside the same batch and never blank. Keeping a
        // stale config across a session swap would be the actual bug.
        sessionConfig = nil
        sessionUsage = nil
        // EXP-1051: the layout goes with the usage it is drawn against — the
        // replay carries the current `context_layout` after its log too.
        sessionContextLayout = nil
        sessionRateLimit = nil
    }

    // MARK: - Last-known Dock slots (#EXP-1538)

    /// LOCAL FIRST (dock.json): the stream's latest-wins slots THE Dock draws
    /// from, as this model last knew them. `SteerSessionStore` keeps them when
    /// it retires a detached model, so the next open draws them in its first
    /// frame instead of after the join replay.
    var dockSlots: SteerDockSlots {
        SteerDockSlots(
            taskList: taskList,
            backgroundTasks: backgroundTasks,
            backgroundFirstSeen: backgroundFirstSeen,
            queued: queued,
            compacting: compacting,
            compactingSince: compacting == nil ? nil : compactingSince,
            rateLimit: sessionRateLimit
        )
    }

    /// Seed a FRESH model with the last-known slots before it dials. The join
    /// replay restates (or clears) every one of them in its one commit
    /// (`commitStaging` → `resetFeed`); an ended session takes none.
    func seedDockSlots(_ slots: SteerDockSlots) {
        guard !isOver, feed.isEmpty else { return }
        backgroundFirstSeen = slots.backgroundFirstSeen
        taskList = slots.taskList
        backgroundTasks = slots.backgroundTasks
        queued = slots.queued
        sessionRateLimit = slots.rateLimit
        if let compaction = slots.compacting {
            compacting = compaction
            armCompactionTimeout(startedAt: slots.compactingSince)
        }
    }

    // MARK: - Compaction strip (EXP-724)

    /// EXP-848: everything that says "work is in flight" — the compaction
    /// strip and the turn slot. Every path that can strand one strands both,
    /// so they are cleared together.
    private func clearLiveWork() {
        clearCompaction()
        turn = AgentTurnSlot()
        // EXP-850: the two §1-§3 slots are live work too — a replay swap or an
        // ended run must not leave a strip or a running card standing.
        backgroundTasks = []
        // EXP-927: the task list is the same kind of live state.
        taskList = []
        workflows = []
        // EXP-861: the queue is device state that only a live run can hold —
        // the join replay carries the current one right after `turn`.
        queued = []
    }

    /// Close the strip and disarm its backstop. Idempotent — every path that
    /// can strand a `started` calls it (the replay swap, the session ending,
    /// teardown).
    private func clearCompaction() {
        compactionTimeoutTask?.cancel()
        compactionTimeoutTask = nil
        compacting = nil
    }

    /// The strip can never stick: a `started` whose `ended` never lands (a
    /// publisher that died mid-compaction, a dropped frame) expires on its own
    /// after `AgentFeed.compactionTimeoutSeconds`. No marker row — nothing
    /// confirms the compaction ever finished.
    ///
    /// `startedAt` is the frame's own `at` stamp (ms) when it carries one: a
    /// replayed `started` from long ago has already used up its budget and
    /// expires at once instead of holding the strip for the full window
    /// (web/desktop parity).
    private func armCompactionTimeout(startedAt: Double?) {
        compactionTimeoutTask?.cancel()
        compactingSince = startedAt ?? Date().timeIntervalSince1970 * 1000
        let age = startedAt.map { max(0, Date().timeIntervalSince1970 - $0 / 1000) } ?? 0
        let delay = max(0, AgentFeed.compactionTimeoutSeconds - age)
        compactionTimeoutTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard let self, !Task.isCancelled else { return }
            self.compactionTimeoutTask = nil
            self.compacting = nil
        }
    }

    // MARK: - Staged join replay (EXP-656)

    private var isStaging: Bool { stagedFrames != nil }

    /// Open (or restart) a staging window. The visible feed is untouched: a
    /// second `activity_reset` means the device superseded the replay we were
    /// buffering, not that the reader should lose what is on screen.
    private func beginStaging() {
        stagedFrames = []
        stagedLocalEchoes = []
        let now = Date()
        stagingStartedAt = now
        lastStagedFrameAt = now
        armStagingWatcher()
    }

    private func stageFrame(_ activity: SteerWireFrame.Activity, seq: Int?) {
        guard activity.event != nil else { return }
        stagedFrames?.append((activity: activity, seq: seq))
        lastStagedFrameAt = Date()
    }

    /// The fallback that ends a markerless replay: one task per staging window,
    /// asking the pure rule (quiet window, hard cap) on each tick. A task per
    /// staged frame would mean thousands of cancel/create pairs per replay.
    private func armStagingWatcher() {
        stagingWatcherTask?.cancel()
        stagingWatcherTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(Self.replayWatchSeconds))
                guard let self, !Task.isCancelled, self.isStaging else { return }
                // #EXP-1623: frames of this replay are still parsing off
                // main; they are not silence, and the clock waits for them.
                guard !self.parsingBurst else { continue }
                let now = Date()
                let startedAt = self.stagingStartedAt ?? now
                guard SteerReplayStaging.shouldCommit(
                    now: now,
                    lastFrameAt: self.lastStagedFrameAt ?? startedAt,
                    startedAt: startedAt,
                    quiet: Self.replayQuietSeconds,
                    max: Self.replayMaxSeconds
                ) else { continue }
                let capped = now.timeIntervalSince(startedAt) >= Self.replayMaxSeconds
                self.commitStaging(why: capped ? "deadline" : "quiet", firstSeq: nil)
                return
            }
        }
    }

    /// Swap the staged replay in as ONE feed change: the old feed and the
    /// replayed one never coexist and the feed is never momentarily empty, so
    /// no scroll observer ever sees the collapse that yanked the reader to the
    /// bottom.
    private func commitStaging(why: String, firstSeq: Int?) {
        guard let staged = stagedFrames else { return }
        stagingWatcherTask?.cancel()
        stagingWatcherTask = nil
        stagedFrames = nil
        stagingStartedAt = nil
        lastStagedFrameAt = nil
        let echoes = stagedLocalEchoes
        stagedLocalEchoes = []
        // Locks that were still waiting for their `answer_ack` when the replay
        // started: a tap made during the staging window must not be undone by
        // the swap (the replay predates it and brings the card back unanswered).
        let carriedLocks = answerTracker.pending
        let carriedLabels = answerTracker.labels

        // One batch (EXP-582): the projections are rebuilt once at the end, not
        // per replayed frame. Nested when the commit runs inside a flush, which
        // is the normal marker/keepalive path.
        let nested = applyingBatch
        applyingBatch = true
        // The oldest visible item's id: replaying the same history from here
        // hands the unchanged prefix the ids it already had, so SwiftUI keeps
        // every row's identity (and the reader's anchor) across the swap. Safe
        // BECAUSE the swap is one transaction — a bare reset used to leave the
        // old rows on screen while the counter rewound, which is a "same row,
        // new content" identity.
        let anchorId = feed.first?.id
        // EXP-783: everything this client holds BELOW the replay's oldest
        // sequence is a prefix the replay does not restate — pages a reader
        // scrolled back to load, which the full swap used to throw away. Kept
        // only when the WHOLE prefix is numbered: an unnumbered row cannot be
        // proved older than the replay, so one of them makes this the full
        // swap it has always been.
        var retained: [AgentFeedItem] = []
        if let firstSeq {
            var split = 0
            while split < feed.count,
                  let seq = seqById[feed[split].id], seq < firstSeq {
                split += 1
            }
            retained = Array(feed[..<split])
        }
        let retainedSeqs = retained.reduce(into: [Int: Int]()) { map, item in
            if let seq = seqById[item.id] { map[item.id] = seq }
        }
        let retainedNextId = retained.last.map { $0.id + 1 }
        let retainedTests = testResults.filter { entry in
            retained.contains { item in
                if case let .tool(_, _, _, _, callId, _, _, _, _, _, _, _, _) = item { return callId == entry.key }
                return false
            }
        }
        resetFeed()
        if let anchorId { nextEventId = anchorId }
        // The retained prefix keeps its rows AND its ids; the replay continues
        // numbering above them, so no row identity is reused.
        if !retained.isEmpty {
            feed = retained
            feedBytes = retained.reduce(0) { $0 + AgentFeed.itemBytes($1) }
            openThoughts = retained.reduce(0) { count, item in
                if case .thought(_, _, true, _, _) = item { return count + 1 }
                return count
            }
            seqById = retainedSeqs
            testResults = retainedTests
            if let retainedNextId { nextEventId = retainedNextId }
        }
        for staging in staged {
            currentSeq = staging.seq
            handleActivityEvent(staging.activity)
        }
        currentSeq = nil
        for text in echoes where !tailCarriesEcho(text) {
            // Not in the replay: re-show it, and re-arm the dedupe so its
            // transcript-derived twin doesn't render a second copy (EXP-78).
            let rowId = takeEventId()
            recentEchoes.append(AgentEcho(
                text: text.trimmingCharacters(in: .whitespacesAndNewlines), at: Date(), rowId: rowId
            ))
            append(.userMessage(id: rowId, text: text, at: Int(Date().timeIntervalSince1970 * 1000)))
        }
        for (key, sentAt) in carriedLocks where feedCarriesQuestion(key) {
            restoreAnswerLock(key, labels: carriedLabels[key] ?? [], sentAt: sentAt)
        }
        applyingBatch = nested
        if !replayed { replayed = true }
        if !nested { reproject() }
        logger.info(
            "replay committed why=\(why, privacy: .public) frames=\(staged.count) items=\(self.feed.count)"
        )
    }

    /// Drop a staged replay and KEEP the visible feed (EXP-656): the socket
    /// went away mid-burst, so the buffer is a partial history of a room this
    /// client is no longer joined to. The next join replays from scratch.
    private func discardStaging() {
        guard isStaging else { return }
        stagingWatcherTask?.cancel()
        stagingWatcherTask = nil
        stagedFrames = nil
        stagingStartedAt = nil
        lastStagedFrameAt = nil
        stagedLocalEchoes = []
        logger.info("staged replay discarded")
    }

    /// Whether the committed feed already ends with this echo — the replay is
    /// authoritative, so anything it carried back must not be duplicated.
    private func tailCarriesEcho(_ text: String) -> Bool {
        let needle = text.trimmingCharacters(in: .whitespacesAndNewlines)
        for item in feed.suffix(Self.echoCap).reversed() {
            guard case let .userMessage(_, existing, _, _) = item else { continue }
            if existing.trimmingCharacters(in: .whitespacesAndNewlines) == needle { return true }
        }
        return false
    }

    private func feedCarriesQuestion(_ lockKey: String) -> Bool {
        feed.contains { $0.question?.lockKey == lockKey }
    }

    /// Re-apply a lock the commit's `resetFeed` cleared, keeping its ORIGINAL
    /// send time so the card expires when it always would have.
    private func restoreAnswerLock(_ lockKey: String, labels: [String], sentAt: Date) {
        answerTracker.markSent(lockKey, labels: labels, at: sentAt)
        let remaining = max(0, Self.answerLockSeconds - Date().timeIntervalSince(sentAt))
        answerExpiryTasks[lockKey]?.cancel()
        answerExpiryTasks[lockKey] = Task { [weak self] in
            try? await Task.sleep(for: .seconds(remaining))
            guard let self, !Task.isCancelled else { return }
            self.answerExpiryTasks[lockKey] = nil
            self.answerTracker.expire(lockKey)
        }
    }

    /// EXP-850: the wire READING lives in `AgentActivityDecoder` (ExpCore,
    /// unit-tested against the engine's own JSON); this switch is only what
    /// each decoded event DOES to the session's state. An unknown kind
    /// decodes to nil and is skipped — never fatal, because a newer desktop
    /// may publish events this build has no renderer for.
    private func handleActivityEvent(_ activity: SteerWireFrame.Activity) {
        guard let decoded = activity.event else { return }
        switch decoded {
        case let .narration(text, messageId, narrationAgent, beforeQuestionId):
            // EXP-483: prose from the withheld ask/plan entry flushes AFTER
            // its already-published card — splice it back above the card.
            // The id is taken up front so the spliced row can be stamped
            // with its wire sequence without a scan for "which one is new"
            // (EXP-783); an anchor that matches nothing falls through with it.
            var splicedId: Int?
            if let anchor = beforeQuestionId {
                let id = takeEventId()
                splicedId = id
                if let out = AgentFeed.spliceBeforeQuestion(
                    feed, anchor: anchor,
                    item: .narration(
                        id: id, text: text,
                        messageId: messageId, subagentId: narrationAgent
                    )
                ) {
                    if let seq = currentSeq { seqById[id] = seq }
                    feed = out
                    // The insert is not at the tail, so re-derive the weight.
                    recountFeedBytes()
                    return
                }
            }
            // EXP-772: consecutive flushes of ONE assistant message are one
            // bubble. A merge consumes no feed id — the row it grew already
            // has one.
            // #EXP-1623: in place — a copy of the feed per fragment made a
            // replay quadratic.
            if AgentFeed.mergeNarration(
                &feed, text: text, messageId: messageId, subagentId: narrationAgent
            ) {
                feedBytes += text.utf8.count
                trimFeed()
                return
            }
            append(.narration(
                id: splicedId ?? takeEventId(), text: text,
                messageId: messageId, subagentId: narrationAgent
            ))
        case let .apiError(message, errorType, subagentId):
            // An inline row, never the rate-limit slot: a transient API error
            // is not a wall.
            append(.apiError(
                id: takeEventId(), message: message,
                errorType: errorType, subagentId: subagentId
            ))
        case let .tool(name, detail, subagentId, callId, toolKind):
            append(.tool(
                id: takeEventId(),
                name: name,
                detail: detail,
                subagentId: subagentId,
                callId: callId,
                toolKind: toolKind
            ))
        case let .toolUpdate(update):
            // EXP-785/786: folded INTO the tool row with that call id — never
            // a row. An id this feed does not hold is dropped.
            // #EXP-1623: patched in place and weighed by its delta — the copy
            // and the full re-count per update made a replay quadratic.
            guard let patched = AgentFeed.applyToolUpdate(&feed, update: update) else { return }
            let at = patched.index
            feedBytes += AgentFeed.itemBytes(feed[at]) - patched.oldBytes
            // A settled `execute`'s output parses as a test run ONCE, here.
            if case let .tool(_, _, _, _, _, kind, settled, _, _, _, output, _, _) = feed[at],
               settled, kind == "execute", let output, !output.isEmpty {
                if let results = Trace.parseTestResults(output) {
                    testResults[update.id] = results
                } else {
                    testResults.removeValue(forKey: update.id)
                }
            }
        case let .diff(diff):
            // Diffs never enter the feed — the latest replaces the previous
            // one behind the pinned "Latest changes" chip.
            latestDiff = diff
        case let .userMessage(text, subagentId, at):
            // A message this client just sent was already echoed locally —
            // skip its transcript-derived twin (EXP-78).
            if consumeEcho(text) { return }
            // EXP-773: a turn addressed to a subagent renders inside that
            // subagent's run, never in the main thread.
            append(.userMessage(id: takeEventId(), text: text, subagentId: subagentId, at: at))
        case let .question(draft):
            // A re-emitted wire id REPLACES its card in place (the desktop
            // augments options it discovers later); anything else appends.
            let before = feed.count
            feed = AgentFeed.upsertQuestion(feed, question: draft.question(id: takeEventId()))
            // Appended: stamp its sequence. Replaced in place: the card's
            // options grew, so re-derive the byte accumulator.
            if feed.count > before, let seq = currentSeq, let id = feed.last?.id {
                seqById[id] = seq
            }
            recountFeedBytes()
        case let .questionResolved(id, askId, answers, dismissed):
            // Collect the retiring cards' lock keys BEFORE they resolve —
            // a retired card has nothing left for the optimistic lock to guard.
            // EXP-820: ALREADY-resolved cards too — a re-answered earlier step
            // resolves a second time, and its fresh pending lock must clear.
            let retiredKeys: [String] = feed.compactMap { item -> String? in
                guard let question = item.question else { return nil }
                if let id { return question.wireId == id ? question.lockKey : nil }
                if let askId { return question.askId == askId ? question.lockKey : nil }
                return question.lockKey
            }
            if let out = AgentFeed.applyQuestionResolved(
                feed, id: id, askId: askId, answers: answers, dismissed: dismissed
            ) {
                feed = out
                // A resolution writes answers into cards anywhere in the
                // transcript; the running byte count is cheaper to re-derive.
                recountFeedBytes()
            }
            for key in retiredKeys { answerTracker.resolve(key) }
        case let .answerAck(id):
            // The desktop injected the answer — the card stays locked for good
            // and a stepper advances to its next step.
            answerTracker.acknowledge(id)
        case let .subagent(subagentId, agentType, status, detail, toolCalls, title, workflowId):
            // EXP-1324: the edges' own `at` stamps give the line its badge.
            if let at = activity.at {
                var span = subagentSpans[subagentId] ?? (start: nil, end: nil)
                if status == .completed {
                    span.end = max(span.end ?? at, at)
                } else if status == .started {
                    span.start = min(span.start ?? at, at)
                }
                subagentSpans[subagentId] = span
                // Prune to the subagents the feed still holds (plus this one,
                // whose row lands below). #EXP-1623: only once the table has
                // doubled — a pass per edge made a replay quadratic.
                if subagentSpans.count > subagentSpansPruneAt {
                    var held: Set<String> = [subagentId]
                    for item in feed {
                        if case let .subagent(_, id, _, _, _, _, _, _) = item { held.insert(id) }
                    }
                    subagentSpans = subagentSpans.filter { held.contains($0.key) }
                    subagentSpansPruneAt = max(32, subagentSpans.count * 2)
                }
            }
            append(.subagent(
                id: takeEventId(),
                subagentId: subagentId,
                agentType: agentType,
                status: status,
                detail: detail,
                // EXP-748: the publisher's count, stamped on the completed
                // edge — it outlives the tool rows replay evicts first.
                toolCalls: toolCalls,
                // EXP-847: what the spawn asked for; absent on older builds.
                title: title,
                // EXP-850/856: a workflow agent's edges nest under its card.
                workflowId: workflowId
            ))
        case let .permission(tool, detail):
            append(.permission(id: takeEventId(), tool: tool, detail: detail))
        // EXP-846/848/850: every LATEST-WINS slot below is about the run's
        // PRESENT, so a page of OLDER transcript folded through this same
        // reducer (`prependingPage`) must never repaint one — web saves and
        // restores them around its fold (`steer-session-store.ts`
        // `prependPage`), which is exactly what skipping them here achieves.
        case let .configState(update):
            // EXP-746: latest-wins STATE, not a row. A malformed frame keeps
            // whatever the chips already show (the fold's contract).
            guard !prependingPage else { return }
            let next = update.applied(to: sessionConfig)
            if next != sessionConfig { sessionConfig = next }
        case let .usage(update):
            guard !prependingPage else { return }
            let next = update.applied(to: sessionUsage)
            if next != sessionUsage { sessionUsage = next }
        case let .contextLayout(update):
            // EXP-1051: latest-wins STATE like the usage it is drawn against —
            // an unreadable frame keeps the layers the sheet already shows.
            guard !prependingPage else { return }
            let next = update.applied(to: sessionContextLayout)
            if next != sessionContextLayout { sessionContextLayout = next }
        case let .rateLimit(update):
            // EXP-784: the fourth slot; an empty/`ok` status clears it.
            guard !prependingPage else { return }
            let next = update.applied(to: sessionRateLimit)
            if next != sessionRateLimit { sessionRateLimit = next }
        case let .turn(edge):
            // EXP-848: latest-wins STATE, never a row. Skipped while an OLDER
            // page folds through this reducer — a turn edge from the far end of
            // the transcript says nothing about what the run is doing now.
            // EXP-850 §5: it also carries the working caption's clock and
            // token count.
            guard !prependingPage else { return }
            turn = AgentFeed.applyTurn(turn, edge: edge)
            // EXP-1324: a turn that ended settles every thought still open (a
            // `started` whose `ended` never came reads settled, no duration).
            if turn.state == .ended, openThoughts > 0 {
                AgentFeed.settleThoughts(&feed)
                openThoughts = 0
            }
        case let .backgroundTasks(tasks):
            // EXP-850 §2: the FULL current list, latest-wins — an empty array
            // closes the strip above the composer.
            guard !prependingPage else { return }
            if tasks != backgroundTasks { backgroundTasks = tasks }
        case let .thought(thoughtId, started, durationMs, subagentId):
            // EXP-1324: the Reasoning line. `ended` patches its `started` row
            // (same wire id) in place; an unmatched `started` appends, an
            // orphan `ended` (evicted, below the window) is dropped (web).
            let wasRunning = AgentFeed.thoughtRunning(feed, thoughtId: thoughtId)
            if let wasRunning {
                // A duplicate `started` changes nothing; an `ended` patches the
                // row, and only one that was still running leaves the count.
                if !started {
                    AgentFeed.applyThought(&feed, thoughtId: thoughtId, started: false, durationMs: durationMs)
                    if wasRunning { openThoughts = max(0, openThoughts - 1) }
                }
            } else if started {
                openThoughts += 1
                append(.thought(
                    id: takeEventId(), thoughtId: thoughtId, running: started,
                    durationMs: durationMs, subagentId: subagentId
                ))
            }
        case let .taskList(entries):
            // EXP-927 §2c: the FULL list, latest-wins — an empty array hides
            // the block. Present-tense state, like the tasks beside it. A
            // frame with no readable list (nil) keeps the current one, as on
            // web/Android/desktop.
            guard !prependingPage, let entries else { return }
            if entries != taskList { taskList = entries }
        case let .queue(messages):
            // EXP-861: the FULL current queue, latest-wins (replace whole) —
            // an empty array clears the strip. Present-tense state, so an
            // older page folding through says nothing about it.
            guard !prependingPage else { return }
            if messages != queued { queued = messages }
            takeBackQueuedEchoes(messages)
        case let .workflow(workflow):
            // EXP-850 §3: latest-wins PER ID. The card patches onto the tool
            // row with the same id, so the projection has to be redone.
            guard !prependingPage else { return }
            workflows = AgentFeed.applyWorkflow(workflows, workflow: workflow)
            // #EXP-1623: a batch (a replay) projects once, at its end.
            if !applyingBatch { reproject() }
        case let .compaction(edge):
            // EXP-724. The strip's state is the pure fold; the marker row is
            // the caller's job because only `ended` writes one — and it writes
            // one even for an UNMATCHED `ended` (codex publishes no start
            // marker for auto-compaction, so the gap still gets explained).
            // The marker ROW is transcript, so an older page still prepends it;
            // the live strip and its backstop are present-tense state.
            if !prependingPage {
                compacting = AgentFeed.applyCompaction(compacting, edge: edge)
                if edge.started {
                    armCompactionTimeout(startedAt: edge.at)
                } else if edge.ended {
                    compactionTimeoutTask?.cancel()
                    compactionTimeoutTask = nil
                }
            }
            if edge.ended {
                append(.compaction(id: takeEventId()))
            }
        }
    }

    /// EXP-1342 (web `takeQueuedEchoes`): an echoed message the device is
    /// HOLDING lives only in the Queued strip until delivery — its echo row
    /// goes, so a Stop handing the strip back to the composer (or a late
    /// delivery) never shows it twice.
    private func takeBackQueuedEchoes(_ messages: [QueuedMessage]) {
        let taken = AgentFeed.takeQueuedEchoes(&recentEchoes, queued: messages)
        guard !taken.isEmpty else { return }
        let takenTexts = Set(taken.map(\.text))
        stagedLocalEchoes.removeAll {
            takenTexts.contains($0.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        let rowIds = Set(taken.compactMap(\.rowId))
        guard !rowIds.isEmpty else { return }
        feed.removeAll { rowIds.contains($0.id) }
        for id in rowIds { seqById.removeValue(forKey: id) }
        recountFeedBytes()
    }

    /// Whether an incoming `user_message` matches a recent local echo —
    /// consumes the matched entry (and evicts expired ones); true = skip it.
    private func consumeEcho(_ text: String) -> Bool {
        let now = Date()
        recentEchoes.removeAll { now.timeIntervalSince($0.at) > Self.echoTTLSeconds }
        let needle = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let index = recentEchoes.firstIndex(where: { $0.text == needle }) else { return false }
        recentEchoes.remove(at: index)
        return true
    }

    private func takeEventId() -> Int {
        defer { nextEventId += 1 }
        return nextEventId
    }

    private func append(_ item: AgentFeedItem) {
        if let seq = currentSeq { seqById[item.id] = seq }
        feed.append(item)
        feedBytes += AgentFeed.itemBytes(item)
        trimFeed()
    }

    /// EXP-783: the feed keeps the WHOLE run — the view renders a WINDOW of
    /// it. Eviction is a memory backstop only (`AgentFeed.feedByteCap`), and
    /// the sequence side-table follows whatever the feed drops.
    private func trimFeed() {
        let before = feed.count
        let trimmed = AgentFeed.trim(feed: feed, bytes: feedBytes)
        guard trimmed.feed.count != before else { return }
        feedHeadTrimmed = true
        for dropped in feed.prefix(before - trimmed.feed.count) {
            seqById.removeValue(forKey: dropped.id)
            if case let .tool(_, _, _, _, callId?, _, _, _, _, _, _, _, _) = dropped {
                testResults.removeValue(forKey: callId)
            }
        }
        feed = trimmed.feed
        feedBytes = trimmed.bytes
    }

    /// Re-derive the byte accumulator — used after the bulk in-place edits
    /// (a resolved question, a replaced card) whose per-item deltas are not
    /// worth passing through.
    private func recountFeedBytes() {
        feedBytes = feed.reduce(0) { $0 + AgentFeed.itemBytes($1) }
        trimFeed()
    }

    /// EXP-783: the oldest wire sequence still on screen — what the next
    /// older-page request is asked relative to. Nil when nothing is numbered
    /// (a publisher older than EXP-783), which is also the signal that paging
    /// is unavailable for this run.
    private var oldestSeq: Int? {
        for item in feed { if let seq = seqById[item.id] { return seq } }
        return nil
    }

    private func disconnectSocket() {
        connected = false
        pendingFrames = []
        discardStaging()
        // A page asked for on that link is not coming either (EXP-795): the
        // device drops the ask with the viewer, so a new one may go out.
        historyRequest = nil
        closeFeed()
    }

    // MARK: - The feed (VAPP-33, #EXP-1528)

    /// The first carrier delivered: the feed is up. Its replay (everything up
    /// to `activity_synced`) follows through `enqueue` as one burst, then the
    /// live frames; a later carrier swap replays the same way.
    private func attachFeed(generation: Int) {
        connected = true
        // EXP-1487: a successful join ends any drop (and its backoff curve).
        dropAt = nil
        failedRedialsSinceDrop = 0
        reconnectAttempts = 0
        startingRetries = 0
        // After a reconnect the replayed transcript is the only copy of a sent
        // message — it must render, so no stale echo may swallow it. The feed
        // is NOT wiped here (EXP-249): the device's `activity_reset` opens the
        // replay's staging window.
        recentEchoes = []
        if let carriers { armStaleWatch(carriers, generation: generation) }
    }

    /// No frame for `staleMs` = re-send `join` on the active carrier (the
    /// device restarts the feed); still nothing one more `staleMs` later = that
    /// carrier is dead: eNet falls back to the relay, a dead relay ends the
    /// feed and the close path redials.
    private func armStaleWatch(_ feed: SteerFeedCarriers, generation: Int) {
        staleTask?.cancel()
        let stale = Double(SteerEnet.staleMs) / 1000
        staleTask = Task { @MainActor [weak self] in
            var rejoinedAt: Date?
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(5))
                guard let self, !Task.isCancelled, self.carriers === feed, generation == self.dialGeneration else { return }
                let last = self.lastFrameAt ?? .distantPast
                if let since = rejoinedAt, last > since {
                    rejoinedAt = nil
                }
                if let since = rejoinedAt {
                    guard Date().timeIntervalSince(since) > stale else { continue }
                    logger.info("feed silent after a re-join, dropping its carrier")
                    rejoinedAt = nil
                    // The next carrier gets its own quiet window.
                    self.lastFrameAt = Date()
                    self.viewStats?.failure("stale")
                    feed.dropActive()
                    continue
                }
                if Date().timeIntervalSince(last) > stale {
                    logger.info("feed silent for \(stale)s, re-joining")
                    rejoinedAt = Date()
                    feed.rejoin()
                }
            }
        }
    }

    private func closeFeed() {
        staleTask?.cancel()
        staleTask = nil
        carriers?.close()
        carriers = nil
    }

    /// - Parameter generation: the `dialGeneration` the closed link belonged
    ///   to. A close from a SUPERSEDED dial is inert — its link is already
    ///   gone, and letting it run would drag a freshly live phase back. Nil for
    ///   the teardowns this model drives itself against whatever link is
    ///   current.
    private func onSocketClosed(generation: Int? = nil) {
        if let generation, generation != dialGeneration { return }
        dialInFlight = false
        guard !stopped else { return }
        logger.info("link closed")
        connected = false
        pendingFrames = []
        discardStaging()
        closeFeed()
        if sawEnd {
            phase = .ended(detail: endDetail)
            return
        }
        // EXP-773: the device that holds the journal is out of reach — still
        // no transcript, so the no-feed rule (and its offline redial, VAPP-169).
        if history != nil {
            applyNoFeed()
            return
        }
        if retryStarting {
            // No feed (VAPP-43): a live row is `starting`, an ended one gets
            // the device-offline copy.
            applyNoFeed()
            return
        }
        if sessionEnded {
            // The row already says ended — the redial loops exit on exactly
            // this, so skip straight to the end state instead of scheduling a
            // retry whose only job is to notice it.
            phase = .ended(detail: nil)
            return
        }
        // Never park on a dead link behind a manual button (EXP-243) —
        // auto-redial on backoff, silently first (EXP-1487).
        feedDown()
    }

    /// EXP-1487 LOST BANNER (fixtures/steer-enet.json `lostBanner`): the
    /// first failure after a join stamps the drop, every later one before the
    /// next successful join is a failed redial. The phase holds (the
    /// transcript stays, no "Connection lost", no `connecting` flash) until
    /// `SteerLostBanner.show`; then the reconnecting close with its banner.
    /// Either way the backoff redials.
    private func feedDown() {
        let now = Date()
        if let since = dropAt {
            failedRedialsSinceDrop += 1
            let downMs = Int(now.timeIntervalSince(since) * 1000)
            if SteerLostBanner.show(downMs: downMs, failedRedials: failedRedialsSinceDrop) {
                phase = .closed(detail: endDetail ?? "Connection lost.", reconnecting: true)
            }
        } else {
            dropAt = now
            failedRedialsSinceDrop = 0
            logger.info("feed dropped, redialing silently")
        }
        scheduleReconnect()
    }

    /// EXP-621: redial without touching the phase — a wake signal found a
    /// nominally live link gone quiet. Flipping to `.closed(reconnecting:)`
    /// would flash a "Reconnecting…" banner and hide the composer for a round
    /// trip. The join replay repaints the feed.
    private func redialNow() {
        retryTask?.cancel()
        reconnectAttempts = 0
        // EXP-1487: the stale link is a drop; a failed redial counts toward
        // the lost banner instead of flipping to `starting`.
        if dropAt == nil {
            dropAt = Date()
            failedRedialsSinceDrop = 0
        }
        retryTask = Task { [weak self] in
            guard let self, !self.stopped, !Task.isCancelled else { return }
            self.resetDialState()
            await self.dial()
        }
    }

    /// Redial while no device feeds the live row — the phase stays
    /// `.starting` so the header doesn't flicker. #EXP-1678: equal-jitter
    /// backoff 3s→30s (`SteerCarrier.startingRetryDelayMs`); a wakeup
    /// (`kick` → `.wakeRetry`) cuts the wait short.
    private func scheduleStartingRetry() {
        retryTask?.cancel()
        let delayMs = SteerCarrier.startingRetryDelayMs(retries: startingRetries)
        startingRetries += 1
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(Int(delayMs)))
            guard let self, !self.stopped, !Task.isCancelled else { return }
            // A row that ended meanwhile still gets this one dial: the device
            // answers with its stored replay (then `bye {history}`), or no
            // device does and the no-feed rule shows the device-offline copy.
            self.resetDialState()
            await self.dial()
        }
    }

    /// Auto-redial after an unexpected drop (EXP-243) — the
    /// phase stays `.closed(reconnecting: true)` across the wait and the dial
    /// so the banner doesn't flicker; a foreground kick() cancels the
    /// wait and dials immediately.
    private func scheduleReconnect() {
        retryTask?.cancel()
        let delay = Self.reconnectDelay(attempt: reconnectAttempts)
        reconnectAttempts += 1
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard let self, !self.stopped, !Task.isCancelled else { return }
            if self.sessionEnded {
                self.retryTask = nil
                self.phase = .ended(detail: nil)
                return
            }
            self.resetDialState()
            await self.dial()
        }
    }

    /// Equal-jitter exponential backoff (web parity): half the capped
    /// exponential delay fixed, half random.
    static func reconnectDelay(attempt: Int) -> Double {
        let capped = min(reconnectMaxSeconds, reconnectBaseSeconds * pow(2, Double(attempt)))
        return capped / 2 + Double.random(in: 0...(capped / 2))
    }

    private func sendText(_ text: String) {
        guard connected, let carriers else { return }
        carriers.send([text])
    }

}

/// An image picked for the steer composer but not sent yet (EXP-511). Held by
/// the MODEL since EXP-621 (the draft has to survive both a reconnect and
/// navigating away) and handed back to `sendSteerImages`; `uploadedId` is
/// stamped on a successful upload so a retry after a mid-batch failure never
/// uploads the same file twice. A top-level type, not nested in the @MainActor
/// model, so it stays free of actor isolation.
struct PendingSteerImage: Identifiable, Equatable, Sendable {
    let id = UUID()
    let data: Data
    let filename: String
    let contentType: String
    var uploadedId: String?
}

/// Wave D: a non-image FILE picked for a steer or start message. Same
/// idempotent upload as `PendingSteerImage` (`uploadedId` stamped once);
/// `uploadedName` = the server's sanitized `filename`, the link text the
/// message carries (`SteerImageMessage.buildMessage`).
struct PendingSteerFile: Identifiable, Equatable, Sendable {
    let id = UUID()
    let data: Data
    let filename: String
    let contentType: String
    var uploadedId: String?
    var uploadedName: String?

    mutating func stamp(_ uploaded: UploadedAttachment) {
        uploadedId = uploaded.id
        uploadedName = uploaded.filename
    }

    /// The message's file line, once uploaded.
    var wireFile: SteerImageMessage.File? {
        uploadedId.map { SteerImageMessage.File(id: $0, name: uploadedName ?? filename) }
    }
}

/// EXP-549/550: the session's host machine as it presents right now — the
/// LIVE `devices` row's label, and whether it stopped heartbeating (nil =
/// unknown: no row, or a devices cursor too stale to vouch for an offline).
struct SessionHostDevice: Equatable {
    let label: String?
    let online: Bool?

    var offline: Bool { online == false }
    var displayLabel: String { label ?? "Desktop" }
}

extension AgentSessionModel: SessionComposerDraft {
    /// EXP-702: a pick attaches to THIS session.
    func uploadAttachment(data: Data, filename: String, contentType: String) async throws -> UploadedAttachment {
        try await attachmentsApi.uploadSessionImage(
            accountId: accountId,
            sessionId: sessionId,
            data: data,
            filename: filename,
            contentType: contentType
        )
    }
}

/// #EXP-1625: one transcript row as an immutable snapshot — the row, keyed
/// by its own id, and the gap above it (the shared ladder keys on the row
/// BEFORE it, so the gap is part of the entry, not a lookup per render).
/// A class: SwiftUI walks the list (ids, keys) on every pass, and copying a
/// reference is a retain where copying the row copied its whole payload. An
/// unchanged row keeps its instance across re-derivations, so the row gate
/// compares by identity first.
final class TranscriptEntry: Identifiable, Equatable {
    let id: Int
    let row: AgentFeedRow
    let gap: AgentTranscriptGap

    init(row: AgentFeedRow, gap: AgentTranscriptGap) {
        self.id = row.id
        self.row = row
        self.gap = gap
    }

    nonisolated static func == (a: TranscriptEntry, b: TranscriptEntry) -> Bool {
        a === b || (a.id == b.id && a.gap == b.gap && a.row == b.row)
    }

    /// The entries for `rows`, reusing `previous`'s instance for every row
    /// that did not move (a diffed update: a live append rebuilds one).
    static func entries(_ rows: [AgentFeedRow], previous: [TranscriptEntry]) -> [TranscriptEntry] {
        var old: [Int: TranscriptEntry] = [:]
        old.reserveCapacity(previous.count)
        for entry in previous { old[entry.id] = entry }
        var out: [TranscriptEntry] = []
        out.reserveCapacity(rows.count)
        var prev: AgentRowClass?
        for row in rows {
            let cur = row.rowClass
            let gap = AgentFeed.transcriptGap(prev: prev, cur: cur)
            if let kept = old[row.id], kept.gap == gap, kept.row == row {
                out.append(kept)
            } else {
                out.append(TranscriptEntry(row: row, gap: gap))
            }
            prev = cur
        }
        return out
    }
}
