import ExpCore
import ExpUI
import Foundation
import GRDB
import os

/// EXP-625: one line per lifecycle event (connect, dial, phase change, kick
/// decision, join-ack timeout, close). Payloads never go in: the whole point of
/// the activity channel is that it is scrubbed, and a log is not.
private let logger = Logger(subsystem: "at.exponential", category: "AgentSessionModel")

/// Viewer side of the steer relay's ACTIVITY channel (EXP-32 — the chat-style
/// "Agent session" screen; apps/steer-relay/src/protocol.ts). Mints a viewer
/// ticket over tRPC, dials the returned ws(s) URL with URLSessionWebSocketTask,
/// joins with {"t":"join","channel":"activity"}, and receives scrubbed
/// {t:'activity', event} frames (narration / tool headlines / questions /
/// subagents / permissions / worktree diffs). EXP-249 removed the PTY mirror
/// from the protocol, so every frame is JSON now — a stray BINARY frame (an old
/// desktop's 0x01 output) is ignored. Steering is message-shaped and fully
/// seamless (EXP-312 — no operator claim, no view/steer perm split; the mint
/// is owner-only, so a live connection just steers): chunked input + a
/// separate \r for prose, and ONE semantic `answer` frame per question card.
/// Mirrors the Android AgentSessionViewModel.
@MainActor @Observable
final class AgentSessionModel {
    enum Phase: Equatable {
        case idle
        case connecting
        case live
        /// The relay reported no_such_session while the synced row still says
        /// running — the desktop is still dialing its publisher socket. The
        /// model auto-redials (fresh ticket) every ~3s until the room is live.
        case starting
        /// The session ended (relay `bye`). Feed retained, input hidden.
        case ended(detail: String?)
        /// Unexpected socket loss / ticket failure. With `reconnecting` the
        /// model auto-redials on jittered exponential backoff (EXP-243 — no
        /// manual Reconnect button); false only for terminal states (steer
        /// disabled on this instance).
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
        case .live: return .live
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
            if case .ended = phase { clearCompaction() }
            // FEED-26: the fallback "last activity" — a run that goes quiet
            // the moment it comes live still needs a clock to count from.
            if phase == .live { markActivity() }
        }
    }
    /// The feed stays visible while disconnected (closed/ended states) and is
    /// cleared ONLY by the relay's `activity_reset` frame (EXP-249) — the relay
    /// sends one to every activity viewer immediately before its join replay,
    /// so the client never has to guess when to wipe. Item shapes and the pure
    /// grouping/resolution rules live in ExpCore's AgentFeed.
    private(set) var feed: [AgentFeedItem] = [] {
        didSet {
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
    /// EXP-783: there is transcript ABOVE the rendered window — rows the feed
    /// already holds, or a page only the device has. What the transcript's
    /// "Load earlier" affordance is gated on.
    private(set) var canLoadEarlier = false
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
    /// EXP-773: where an ENDED run's transcript is coming from. The relay has
    /// no room for a finished session, so it asks the device that ran it to
    /// republish its on-disk journal; these are the three answers. Nil on a
    /// live session and once the transcript has arrived.
    enum HistoryState: Equatable {
        /// The relay woke the device and is waiting for it to publish.
        case pending
        /// The machine that holds the journal is not reachable. Terminal:
        /// nothing to redial for, the file is on that disk.
        case deviceOffline
        /// The device answered and has no journal for this run. Terminal.
        case unavailable

        /// Nothing is going to change on its own — stop dialing.
        var isTerminal: Bool { self != .pending }
    }

    private(set) var history: HistoryState?
    /// EXP-773: the machine a Resume of this ENDED run would go to, or nil
    /// when there is none (a live run, someone else's, a machine that is away
    /// or too old to advertise `resume-run`). Display gating only — the
    /// server re-decides the same rule.
    private(set) var resumeDevice: SteerDevice?
    /// EXP-724: a context compaction is running on the host agent — the
    /// composer grows an indeterminate strip for as long as this is non-nil,
    /// so the 10–170s of silence reads as work instead of a hang. Set by the
    /// `compaction` activity event and cleared by its `ended` edge, the replay
    /// swap, the end of the session, and a backstop timer.
    private(set) var compacting: AgentCompaction?
    /// FEED-26: when this run's feed last CHANGED while live, falling back to
    /// the moment the phase became live. What `staleActivityMinutes` counts
    /// from.
    private(set) var lastActivityAt: Date?
    /// FEED-26: the clock `staleActivityMinutes` reads, re-stamped every 30s.
    /// Silence is the ABSENCE of frames, so without a tick nothing would ever
    /// redraw the header while a run goes quiet (the host-liveness pattern).
    private(set) var activityNow = Date()
    /// EXP-746: the agent's live configuration behind the composer chips —
    /// model, effort, mode, and the agent's own slash commands. Latest-wins
    /// STATE beside the feed, never a row: the relay replays the newest
    /// `config_state` after its log, so a join and a reconnect both repaint
    /// from one frame.
    private(set) var sessionConfig: AgentSessionConfig?
    /// EXP-746: this run's context window and spend, as the engine last
    /// measured them. A different quantity from `agentUsage` (the machine's
    /// rate-limit windows), so it lives beside it and renders in its own
    /// block.
    private(set) var sessionUsage: AgentSessionUsage?
    /// EXP-784: the agent's rate-limit window, the fourth latest-wins slot.
    /// Nil = not limited (or cleared by an empty/`ok` status).
    private(set) var sessionRateLimit: AgentSessionRateLimit?
    /// The synced coding_sessions row — flips to ended via Electric.
    private(set) var session: CodingSessionEntity?
    /// EXP-549/550: the host machine as it presents right now — the LIVE
    /// `devices` row's label (renames land; the session's `device_label` is a
    /// start-time snapshot) and whether that machine stopped heartbeating.
    private(set) var hostDevice = SessionDevicePresentation(label: nil, online: nil)
    /// EXP-484: how much of its rate-limit window the agent running THIS
    /// session has used, off the host machine's synced report. Nil whenever
    /// there is nothing honest to draw — a finished run, a pre-EXP-484 row
    /// with no agent, no matching devices row, or numbers older than the
    /// freshness window (all decided by `AgentUsagePresentation.sessionUsage`).
    /// Recomputed on the same three inputs as `hostDevice`.
    private(set) var agentUsage: SessionAgentUsage?
    /// EXP-688: the host machine's sign-in status for THIS session's agent,
    /// for the Usage sheet's caption. Read-only visibility off the same synced
    /// row — nothing here holds or forwards a credential.
    private(set) var agentAccount: AgentAccount?
    /// EXP-678: the issue whose PR the Merge pill merges — this session's own
    /// issue, or, for an issueless + actionless batch run in review, the
    /// representative issue of the batch PR its branch names (EXP-535). Nil
    /// for action runs and whenever there is nothing to merge through.
    private(set) var mergeIssue: IssueEntity?
    /// EXP-734: WHAT the Merge pill merges — `mergeIssue`'s PR, or (an action
    /// or chat run that opened its own issue-less PR) this session row's own.
    /// Nil whenever there is nothing to merge.
    private(set) var mergeTarget: MergeTarget?
    /// Kill-switch failure (EXP-268), surfaced as an inline banner — cleared
    /// on each attempt. Success needs no local state: the synced row flips to
    /// `ended` and the view already reacts.
    private(set) var killError: String?
    /// An image-carrying steer message is uploading (EXP-511) — the composer
    /// dims its send button until the whole batch is out.
    private(set) var steerSending = false
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
    let draftEditor = IssueEditorModel()

    /// The draft as text. Every existing reader and writer of the draft goes
    /// through here unchanged — including `sendMessage`, which puts THIS on the
    /// wire: `plainText`, never the markdown serializer, because a steer
    /// message is chat prose and must go out exactly as typed.
    var draftText: String {
        get { draftEditor.plainText }
        set { draftEditor.setPlainText(newValue) }
    }
    /// EXP-511: images picked for the next steer message, shown as a strip
    /// above the input row until they are sent or removed.
    var pendingImages: [PendingSteerImage] = []

    /// EXP-724: the slash-command rows the composer's menu should show for the
    /// current draft — empty whenever the menu must not open (the pure rule
    /// lives in ExpCore's SlashCommands, mirrored ×4).
    var slashMatches: [SlashCommand] {
        // EXP-746: the contract catalog UNION whatever the agent itself
        // advertised on `config_state` (contract first, dedupe by name). An
        // agent-less run that published one is EXTERNAL, so it contributes no
        // contract rows at all (`agentId(_:acp:)`).
        SlashCommands.matches(
            draft: draftText,
            agent: catalogAgent,
            extra: sessionConfig?.commands ?? []
        )
    }

    /// EXP-772: the composer's ONE chip — the agent's mode. Nil until the
    /// first `config_state`, and on every run whose agent advertises no modes.
    var modeChip: AgentModeChip? {
        AgentFeed.modeChip(sessionConfig)
    }

    /// EXP-746: the id the catalog is keyed on — this run's agent, or, for an
    /// agent-less run that publishes a `config_state`, the external one that
    /// cannot name itself on the wire.
    var catalogAgent: String {
        SlashCommands.agentId(session?.agent, acp: sessionConfig != nil)
    }

    /// The catalog command the draft would SEND as, if any — what the confirm
    /// gate reads before a `/clear` goes out.
    var pendingSlashCommand: SlashCommand? {
        SlashCommands.command(for: draftText, agent: catalogAgent)
    }

    /// The draft as it would be sent — what the send button's enablement and
    /// the empty-message guard read.
    var trimmedDraft: String {
        draftText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Over as far as this client can tell: an explicitly `ended` row, or a
    /// row that VANISHED. The model is always constructed with a real row, so
    /// nil means it was deleted (stale rows get swept) or left this client's
    /// sync scope — either way it can never report `ended` itself, and every
    /// retry loop exits on this, so treating nil as "still running" would keep
    /// them dialing forever.
    var sessionEnded: Bool {
        guard let session else { return true }
        return session.status == DomainContract.codingSessionStatusEnded
    }

    /// EXP-621: the session is finished as far as this screen is concerned —
    /// the relay said `bye` or the synced row ended/vanished. The ONLY state
    /// that retires the composer: every other one (connecting, starting,
    /// reconnecting) keeps it on screen with sending disabled, so a draft is
    /// never lost to a hiccup. Also what SteerSessionStore reaps on.
    var isOver: Bool {
        if case .ended = phase { return true }
        return sessionEnded
    }

    /// Whether a steer can go out right now — a live socket on a live session.
    /// `connected` matters beyond the phase: a silent slow-consumer redial
    /// (4008) deliberately keeps `phase == .live` while the socket is briefly
    /// down, and the send button should dim honestly for that gap instead of
    /// offering a tap that would no-op.
    var canSteer: Bool { phase == .live && connected && !sessionEnded }

    /// EXP-550: the host machine is offline while the run is still coding —
    /// the session is PAUSED, not ended (it resumes when the machine comes
    /// back), so the viewer shows that instead of an endless "waiting for the
    /// live stream". Mirrors `SessionDevicePresentation.isPaused`: an
    /// in_review/ended row is past caring where its machine is.
    var hostDeviceOffline: Bool {
        guard hostDevice.offline, let session, !sessionEnded else { return false }
        return session.status == DomainContract.codingSessionStatusRunning
    }

    /// Whether this viewer may kill the session (EXP-268): a live (not-ended)
    /// synced row owned by the caller — everything about a live session is
    /// owner-only (EXP-312). Display gating only — the server enforces the
    /// same rule again.
    var canKill: Bool {
        guard !sessionEnded else { return false }
        guard let currentUserId else { return false }
        return session?.userId == currentUserId
    }

    /// EXP-678: whether the Merge pill shows — merging always ends the run too
    /// (EXP-498), so it only offers while there IS an open PR on a session
    /// this screen still considers live. The button then vanishes on its own
    /// when the merged `pr_state` and the ended row sync back. EXP-734: that
    /// PR may be the run's OWN issue-less one, so the target decides.
    var canMerge: Bool {
        mergeTarget != nil && !isOver
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
    /// A viewer that joined a long-running session holds only the relay's
    /// replay TAIL (`truncated` on `activity_synced` is how it knows), and the
    /// pages below it exist only in the device's journal.
    private func requestOlderPage() -> Bool {
        guard historyRequest == nil, !historyExhausted, historyTruncated else { return false }
        guard let before = oldestSeq, before > 0 else {
            historyExhausted = true
            canLoadEarlier = false
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
    private func prependPage(_ events: [[String: Any]], seqs: [Int]) {
        guard !isStaging, !events.isEmpty else { return }
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
            canLoadEarlier = false
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
        canLoadEarlier = AgentFeed.canLoadEarlier(
            windowStart: start, historyTruncated: historyTruncated,
            historyExhausted: historyExhausted, connected: connected
        )
        let next = AgentFeed.rows(feed, from: start)
        rows = next
        subagents = next.compactMap { row in
            if case let .subagentRun(run) = row { return run }
            return nil
        }
        let active = AgentFeed.activeQuestionIds(feed)
        activeQuestionIds = active
        activeCards = feed.compactMap { item in
            guard let question = item.question, active.contains(question.id) else { return nil }
            return question
        }
    }

    /// The composer's placeholder. EXP-820: always the generic prompt — the
    /// composer HIDES while a card is pending (`cardPending`), and a card's
    /// free answer is an inline field on the card itself.
    var composerPlaceholder: String { AgentFeed.composerPlaceholder }

    /// EXP-820: a card is waiting on THIS steerer — live, not ended, and an
    /// unresolved question card in the feed. The session view drops the
    /// composer band while this holds (the draft stays here, so it is back
    /// intact once the card resolves).
    var cardPending: Bool {
        phase == .live && !sessionEnded && !activeQuestionIds.isEmpty
    }

    /// EXP-790: the agent is actively working — live and nothing waiting on
    /// the user (no active card, synced `needs_input` clear, no compaction
    /// running). Web `working` parity: what swaps the empty composer's send
    /// glyph for Stop.
    var agentWorking: Bool {
        phase == .live && !sessionEnded && activeQuestionIds.isEmpty
            && session?.needsInput != true && compacting == nil
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
    private let codingSessionId: String
    private let currentUserId: String?
    private let steerApi: SteerApi
    private let attachmentsApi: AttachmentsApi
    private let db: DatabaseManager

    private var task: URLSessionWebSocketTask?
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
    /// EXP-783: the publisher's wire sequence per feed row, by row id. A side
    /// table rather than a case field on `AgentFeedItem`: nothing that renders
    /// a row cares, and every pattern match over the enum would have had to
    /// change. It is the only monotonic anchor on the wire — what lets a join
    /// replay be spliced onto a prefix already on screen, and what an
    /// older-page request is addressed relative to.
    private var seqById: [Int: Int] = [:]
    /// The wire sequence of the event being folded in right now, so `append`
    /// can stamp it without threading it through every branch.
    @ObservationIgnored private var currentSeq: Int?
    /// EXP-783: the `history_page` request in flight — a chunk for any other
    /// id is not ours; whether the relay said its log is a TAIL (so there IS
    /// older transcript to ask the device for); and whether a page came back
    /// with nothing new (stop asking).
    @ObservationIgnored private var historyRequest: String?
    @ObservationIgnored private var historyRequests = 0
    private var historyTruncated = false
    private var historyExhausted = false
    /// Locally-echoed sent messages awaiting their transcript-derived
    /// `user_message` event (EXP-78 dedupe).
    private var recentEchoes: [(text: String, at: Date)] = []
    private var retryTask: Task<Void, Never>?
    /// Consecutive failed reconnect dials — indexes the backoff curve; reset
    /// on a successful (live) connection and on an explicit connect().
    private var reconnectAttempts = 0
    /// Monotonic dial id — each dial() invalidates the ones before it, so a
    /// dial that was superseded mid-await (a foreground connect() racing a
    /// fired backoff retry, EXP-243) can't install a second socket or stomp
    /// the winner's phase.
    private var dialGeneration = 0
    /// EXP-625: a dial is between its entry and its outcome (mint failure,
    /// disabled ticket, first frame, or a close). Together with an armed
    /// `retryTask` this is `dialActive`, the ONE thing a wake signal gates on:
    /// the phase lies (a wedged mint sits at `.connecting` forever), a live
    /// dial does not.
    private var dialInFlight = false
    /// When the current dial started, and when the last frame arrived on it.
    /// `lastFrameAt == nil` means the relay has not answered this dial's join.
    private var dialStartedAt: Date?
    private var lastFrameAt: Date?
    /// Fires if the relay never answers the join (EXP-625). The relay ALWAYS
    /// answers one (activity_reset + replay, or no_such_session then a close),
    /// so silence past the deadline means the socket is dead in a way no
    /// receive callback will ever report.
    private var joinAckTask: Task<Void, Never>?
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
    /// EXP-678: the rows behind `mergeIssue` — the session's own issue, or
    /// (batch runs) every issue + board the batch PR resolution scopes over.
    private var mergeObservationTask: Task<Void, Never>?
    private var mergeIssueRows: [IssueEntity] = []
    private var mergeBoardRows: [BoardEntity] = []
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
    /// EXP-582: inbound relay frames waiting for the next flush. The socket's
    /// receive callback hops to the main actor ONCE PER FRAME, and with every
    /// frame mutating `feed` directly each one bought a full SwiftUI render
    /// plus a `scrollTo` — a join replay of a long history (the relay replays
    /// its whole activity log frame by frame) froze the UI and heated the
    /// device for the duration. Frames now queue here and are applied in one
    /// batch per flush, so the replay costs one render per batch.
    @ObservationIgnored private var pendingFrames: [String] = []
    @ObservationIgnored private var flushScheduled = false
    /// Set while a flush applies its batch — the projections are rebuilt once
    /// at the end instead of after every frame.
    @ObservationIgnored private var applyingBatch = false
    /// EXP-656: the activity events of an in-flight join replay. nil = not
    /// staging; non-nil (even empty) = the visible feed is frozen and every
    /// `activity` frame buffers here until the replay commits in ONE pass.
    @ObservationIgnored private var stagedFrames: [(event: [String: Any], seq: Int?)]?
    @ObservationIgnored private var stagingStartedAt: Date?
    @ObservationIgnored private var lastStagedFrameAt: Date?
    /// The one task that watches an in-flight staging window (quiet timeout +
    /// hard cap, both decided by SteerReplayStaging.shouldCommit).
    @ObservationIgnored private var stagingWatcherTask: Task<Void, Never>?
    /// Messages this client sent WHILE a replay was staging: the replay
    /// predates them, so the commit re-appends whatever it didn't carry back.
    @ObservationIgnored private var stagedLocalEchoes: [String] = []

    // Relay rejects input frames > 8 KiB; chunk pastes well under that. The
    // cap is measured in UTF-16 code units (the relay validates against JS
    // string length), so chunking counts UTF-16 units too — counting Swift
    // Characters (grapheme clusters, up to 11 units each) could serialize a
    // chunk past the cap and get the frame silently dropped (REV-15).
    private static let inputChunkUtf16 = 4096
    /// How long an optimistic answer lock holds without an `answer_ack` or a
    /// `question_resolved` — long enough to cover a slow desktop injection,
    /// short enough that a lost frame doesn't strand the card. Web/Android
    /// parity (ANSWER_ACK_TIMEOUT_MS, EXP-249). Derived from the desktop's
    /// worst-case ack budget (EXP-347): ANSWER_RETRY_TTL 4s + ANSWER_SETTLE 2s
    /// + PLAN_SUBMIT_PROBE 0.5s + ~1.5s tick/relay margin — move all three in
    /// lockstep.
    private static let answerLockSeconds: Double = 8
    /// EXP-656: a join replay is STAGED, not applied — see SteerReplayStaging.
    /// The relay's `activity_synced` marker ends it; these bound the fallback
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
    /// Redial cadence while the desktop's publisher socket is still starting.
    private static let startingRetrySeconds: Double = 3
    /// Auto-reconnect backoff after an unexpected drop (EXP-243): jittered
    /// exponential 3s→30s, mirroring the web viewer's starting retry — the
    /// jitter desyncs a herd of viewers all foregrounding at once.
    private static let reconnectBaseSeconds: Double = 3
    private static let reconnectMaxSeconds: Double = 30
    /// How long a dial waits for the relay's answer to its join before treating
    /// the socket as dead (EXP-625). Generous: the join answer carries the full
    /// activity replay, and a cold relay behind a slow network takes a moment.
    private static let joinAckSeconds: Double = 15
    /// A nominally live socket that has delivered nothing for this long is
    /// suspect, so a wake signal redials it silently rather than trusting the
    /// phase. The relay sends every joined viewer a `keepalive` frame every
    /// 15s (EXP-648), so this is three missed ticks: an agent parked on a
    /// question or a plan approval produces no frames of its own, and must
    /// not read as a dead socket.
    private static let liveStaleSeconds: Double = 45
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
        session: CodingSessionEntity,
        currentUserId: String?,
        steerApi: SteerApi,
        attachmentsApi: AttachmentsApi,
        db: DatabaseManager
    ) {
        self.accountId = accountId
        self.codingSessionId = session.id
        self.currentUserId = currentUserId
        self.steerApi = steerApi
        self.attachmentsApi = attachmentsApi
        self.db = db
        self.session = session
        // Snapshot-only until the devices observation lands (EXP-549).
        self.hostDevice = SessionDevicePresentation.resolve(session: session, devices: [])
    }

    /// Bind the synced session row and auto-connect once when presented;
    /// drops after that auto-reconnect (EXP-243).
    func start() {
        startObservingSession()
        startObservingHostDevice()
        startObservingMergeIssue()
        startObservingMentionMembers()
        configureDraftEditor()
        startActivityClock()
        if phase == .idle { connect() }
    }

    /// Dial (or re-dial, with a fresh ticket) the relay room.
    ///
    /// - Parameter force: dial even from `.connecting` (EXP-625). That phase
    ///   used to be an unconditional early-return, which is exactly what made a
    ///   wedged dial unrecoverable: a mint that never returns, or a socket the
    ///   relay never answered, leaves the model at `.connecting` with nothing
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
            // Live and fresh: still ping-probe (EXP-243), because a socket the
            // OS killed while suspended reports nothing until the next receive
            // fails, which may never happen.
            guard phase == .live else { return }
            task?.sendPing { [weak self] error in
                guard error != nil else { return }
                Task { @MainActor in
                    guard let self, self.connected else { return }
                    logger.info("ping probe failed, treating the socket as closed")
                    self.disconnectSocket()
                    self.onSocketClosed()
                }
            }
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
    /// (the relay replays the full activity log on join, rebuilding the feed).
    /// Since EXP-621 the socket outlives the screen — SteerSessionStore only
    /// shuts a model down when it retires it — so this is a no-op on the
    /// normal navigate-away-and-back path, kept for the paths that do stop.
    func resume() {
        guard stopped else { return }
        startObservingSession()
        startObservingHostDevice()
        startObservingMergeIssue()
        startActivityClock()
        connect()
    }

    /// Tear everything down. Called by SteerSessionStore when it retires this
    /// session (over, evicted, or the account went away) — NOT when the screen
    /// merely goes off-view (EXP-621).
    func shutdown() {
        logger.info("shutdown")
        stopped = true
        retryTask?.cancel()
        retryTask = nil
        joinAckTask?.cancel()
        joinAckTask = nil
        dialInFlight = false
        lastFrameAt = nil
        cancelAnswerExpiries()
        clearCompaction()
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
        mergeObservationTask?.cancel()
        mergeObservationTask = nil
        mentionObservationTask?.cancel()
        mentionObservationTask = nil
        connected = false
        pendingFrames = []
        discardStaging()
        // Reset to the pre-connection state so a later resume() can redial: the
        // socket is gone, so leaving phase at .live/.connecting would make
        // connect()'s guard early-return and the reopened view would show a
        // stale "live" status over a dead socket (EXP-221). The normal
        // socket-drop path sets .closed itself and never routes through here.
        phase = .idle
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
    }

    // MARK: - Kill switch (EXP-268)

    /// Force-end the session: `steer.killSession` flips the synced row to
    /// `ended` (the desktop watches its own row over Electric, so the run
    /// aborts even when the relay is unreachable) and best-effort fans a kill
    /// through the relay so the run tears down immediately. On success
    /// nothing changes locally — the synced row flips and the view reacts.
    func killSession() async {
        killError = nil
        do {
            try await steerApi.killSession(accountId: accountId, codingSessionId: codingSessionId)
        } catch {
            killError = error.trpcUserMessage
        }
    }

    // MARK: - Steering (message-shaped; owner-only — the mint refuses others)

    /// Send one message to the agent: the text (chunked ≤4 KiB), then a
    /// SEPARATE `\r` frame — bundled into one write TUI apps treat the
    /// trailing return as a paste, which inserts instead of submitting.
    ///
    /// Returns whether the message actually went out (EXP-621): the composer
    /// stays usable while the socket is down, and a caller that cleared the
    /// draft on this no-op wiped it with nothing sent.
    ///
    /// EXP-820: the text is ALWAYS a message. A pending card's free answer is
    /// its own inline field (`sendAnswer` with `text`, or `answerThenSend` for
    /// a plan's feedback) — the composer no longer routes into cards, it hides
    /// while one is pending.
    @discardableResult
    func sendMessage(_ text: String) -> Bool {
        guard !text.isEmpty, connected else { return false }
        // Chunk by UTF-16 code units, never splitting a surrogate pair —
        // web parity (agent-session.tsx extends the boundary by one unit
        // when it would land mid-pair; 4097 units still sit well under the
        // relay's 8 KiB cap).
        let units = Array(text.utf16)
        var start = 0
        while start < units.count {
            var end = min(start + Self.inputChunkUtf16, units.count)
            if end < units.count, UTF16.isLeadSurrogate(units[end - 1]) {
                end += 1
            }
            let chunk = String(decoding: units[start..<end], as: UTF16.self)
            start = end
            let frame: [String: Any] = ["t": "input", "data": chunk]
            if let data = try? JSONSerialization.data(withJSONObject: frame),
               let json = String(data: data, encoding: .utf8) {
                sendText(json)
            }
        }
        sendText(#"{"t":"input","data":"\r"}"#)
        // Local echo (EXP-78): show the sent message immediately; its
        // transcript-derived `user_message` event is deduped via the FIFO.
        recentEchoes.append((text: text.trimmingCharacters(in: .whitespacesAndNewlines), at: Date()))
        if recentEchoes.count > Self.echoCap {
            recentEchoes.removeFirst(recentEchoes.count - Self.echoCap)
        }
        append(.userMessage(id: takeEventId(), text: text))
        // EXP-656: sent while a replay stages — the replay predates it, so the
        // commit re-appends it unless the replay happened to carry it back.
        if isStaging { stagedLocalEchoes.append(text) }
        return true
    }

    /// Send a steer message carrying attached images (EXP-511): upload every
    /// not-yet-uploaded image to THIS SESSION, then send ONE message composed
    /// of the text plus a markdown embed per attachment (the host device
    /// downloads each embed and hands the agent a local file path).
    ///
    /// EXP-702: the upload goes to `/api/sessions/{id}/files`, not to the
    /// issue's — a steered screenshot is not an issue attachment, and a batch
    /// or action run (no issue at all) can carry images too.
    ///
    /// Returns nil once the message is out; on failure it returns the images
    /// with whatever ids were already stamped, so the caller can keep the strip
    /// and a retry re-uploads only the rest.
    func sendSteerImages(_ text: String, images: [PendingSteerImage]) async -> [PendingSteerImage]? {
        let sessionId = codingSessionId
        guard connected else {
            steerImageError = "Not connected. Wait for the session to reconnect."
            return images
        }
        steerSending = true
        steerImageError = nil
        defer { steerSending = false }
        var pending = images
        for index in pending.indices where pending[index].uploadedId == nil {
            do {
                let uploaded = try await attachmentsApi.uploadSessionImage(
                    accountId: accountId,
                    sessionId: sessionId,
                    data: pending[index].data,
                    filename: pending[index].filename,
                    contentType: pending[index].contentType
                )
                pending[index].uploadedId = uploaded.id
            } catch {
                steerImageError = error.localizedDescription
                return pending
            }
        }
        // The socket can drop across the uploads; sendMessage would silently
        // no-op and the composer would clear with nothing sent.
        guard connected else {
            steerImageError = "Not connected. Wait for the session to reconnect."
            return pending
        }
        guard sendMessage(SteerImageMessage.build(
            text: text, attachmentIds: pending.compactMap(\.uploadedId)
        )) else {
            steerImageError = "Not connected. Wait for the session to reconnect."
            return pending
        }
        return nil
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
        sendMessage(message)
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

    /// EXP-790: stop the agent's current turn — the composer's Stop glyph
    /// while the agent works and the field is empty. Fire-and-forget like
    /// `sendMode`: the turn's own `idle` edge is the confirmation. Same
    /// `canSteer` gate, so a paused or ended run offers no dead tap.
    func sendInterrupt() {
        guard canSteer else { return }
        send(frame: ["t": "interrupt"])
    }

    private func send(frame: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: frame),
              let json = String(data: data, encoding: .utf8) else { return }
        sendText(json)
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
        let id = codingSessionId
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity.filter(Column("id") == id).fetchOne(db)
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
                    for try await row in observation.values(in: pool) {
                        guard let self else { return }
                        // A nil row is published (not swallowed): the row is
                        // gone, which `sessionEnded` reads as ended. Holding
                        // the last known copy instead left the retry loops
                        // waiting for an `ended` status a deleted row can
                        // never report.
                        self.session = row
                        // EXP-724: the run is over (or gone) — no `ended`
                        // compaction frame will ever arrive for it.
                        if self.sessionEnded { self.clearCompaction() }
                        self.rebuildHostDevice()
                        // EXP-678: a batch run's Merge target only appears
                        // once THIS row flips to in_review (the pr_open
                        // transaction), long after its issue rows landed.
                        // EXP-734: an action or chat run's own PR is stamped
                        // on THIS row too, so its pill lives or dies here.
                        self.rebuildMergeTarget()
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
                        self.deviceRows = rows
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
        lastActivityAt = now
        activityNow = now
    }

    private func rebuildHostDevice() {
        guard let session else { return }
        let now = Date()
        // EXP-656: a `last_seen_at` we haven't refreshed since before the
        // suspension is ignorance, not evidence — presence resolves to unknown
        // (never "Paused") until the devices shape has polled inside its
        // contract window.
        hostDevice = SessionDevicePresentation.resolve(
            session: session, devices: deviceRows, now: now,
            devicesFresh: DeviceFreshness.isTrustworthy(
                devicesPolledAt: SyncFreshness.shared.devicesPolledAt(accountId: accountId),
                now: now
            )
        )
        agentUsage = AgentUsagePresentation.sessionUsage(
            session: session, devices: deviceRows, now: now
        )
        // Same devices-row match `sessionUsage` makes (the stamped device id,
        // preferring the session owner's own row) — only the account map is
        // read, and only for the agent the run uses.
        agentAccount = agentUsage.flatMap { usage -> AgentAccount? in
            let byId = deviceRows.filter { $0.deviceId == session.deviceId }
            let row = byId.first { $0.userId == session.userId } ?? byId.first
            return AgentUsagePresentation.parseAccounts(row?.agentAccounts)?[usage.agent]
        }
        // EXP-773: Resume moved from the list row into this screen's header,
        // so the machine a Resume would go to is resolved here — the same ×4
        // rule the lists used (own ended run, its own machine, online and
        // `resume-run`-capable), re-decided on every heartbeat.
        resumeDevice = RunResume.target(
            for: session,
            devices: deviceRows.map {
                SteerDevice(entity: $0, now: now, currentUserId: currentUserId)
            },
            currentUserId: currentUserId
        )
    }

    // MARK: - Composer autocomplete (EXP-802)

    /// Point the draft editor's three typeaheads at this run's team.
    ///
    /// `#` refs resolve through the `.team` scope the steering feed's chips
    /// already use (EXP-760) — a run may be issue-less (batch, action, chat),
    /// so there is no issue or board to derive a team from. `:` emoji need
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
        draftEditor.issueRefSearch = { query in
            IssueRefLookup.search(query, scope: scope, db: database, accountId: account)
        }
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

    // MARK: - Merge target (EXP-678)

    /// Watch whatever the Merge pill would merge through. WHICH query that is
    /// is decided once, off the row the model was constructed with: a
    /// session's `issue_id` and `action_name` never change over its life.
    /// EXP-734: an action run observes no issue rows — its own PR rides the
    /// SESSION row, so the session observation's `rebuildMergeTarget` is the
    /// only input it needs.
    private func startObservingMergeIssue() {
        guard mergeObservationTask == nil else { return }
        guard let session else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        if let issueId = session.issueId {
            let observation = ValueObservation.tracking { db in
                try IssueEntity.filter(Column("id") == issueId).fetchOne(db)
            }
            mergeObservationTask = Task { [weak self] in
                // Same one-shot re-subscribe loop as the session observation
                // (EXP-410): a dead stream would freeze the PR state at its
                // last value, leaving a Merge button up over a merged PR.
                while !Task.isCancelled {
                    do {
                        for try await row in observation.values(in: pool) {
                            guard let self else { return }
                            self.mergeIssue = row
                            self.rebuildMergeTarget()
                        }
                        return
                    } catch is CancellationError {
                        return
                    } catch {
                        try? await Task.sleep(for: .seconds(1))
                    }
                }
            }
        } else if session.actionName == nil {
            // A batch run carries no issue linkage, so its PR resolves
            // client-side off the team's open batch PRs, keyed on the branch
            // the pr_open flip stamped (EXP-535/545, BatchPrResolution).
            let observation = ValueObservation.tracking { db -> ([IssueEntity], [BoardEntity]) in
                (try IssueEntity.fetchAll(db), try BoardEntity.fetchAll(db))
            }
            mergeObservationTask = Task { [weak self] in
                while !Task.isCancelled {
                    do {
                        for try await (issues, boards) in observation.values(in: pool) {
                            guard let self else { return }
                            self.mergeIssueRows = issues
                            self.mergeBoardRows = boards
                            self.rebuildMergeTarget()
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
    }

    /// Re-derive what the Merge pill merges. Several inputs move
    /// independently — the observed issue row, a batch run's issue/board rows,
    /// and the session's own row (the pr_open transaction flips its status,
    /// and EXP-734 stamps an issue-less run's own PR right onto it) — so every
    /// observer calls this.
    private func rebuildMergeTarget() {
        guard let session else {
            mergeTarget = nil
            return
        }
        var openBatchPrs: [IssueEntity] = []
        // A batch run carries no issue linkage: its PR resolves client-side
        // off the team's open batch PRs (EXP-535/545). A chat run runs through
        // the same arm and simply matches nothing.
        if session.issueId == nil, session.actionName == nil {
            // Issues don't sync `team_id` — the team's synced board ids are
            // the scope, same as AgentsViewModel's rebuild.
            let teamBoardIds = Set(mergeBoardRows.filter { $0.teamId == session.teamId }.map(\.id))
            openBatchPrs = BatchPrResolution.openBatchPrs(
                issues: mergeIssueRows, teamBoardIds: teamBoardIds
            )
            mergeIssue = session.status == DomainContract.codingSessionStatusInReview
                ? BatchPrResolution.resolve(
                    sessionBranch: session.branch, openBatchPrs: openBatchPrs
                )
                : nil
        }
        mergeTarget = MergeTargetResolution.resolve(
            session: session, issue: mergeIssue, openBatchPrs: openBatchPrs
        )
    }

    // MARK: - Connect lifecycle

    private func resetDialState() {
        sawEnd = false
        retryStarting = false
        endDetail = nil
    }

    private func dial() async {
        // Whatever the last dial left open is superseded here and now: closing
        // it before bumping the generation means the relay drops that viewer
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
        joinAckTask?.cancel()
        joinAckTask = nil
        let ticket: SteerTicket
        do {
            ticket = try await steerApi.mintViewerTicket(accountId: accountId, codingSessionId: codingSessionId)
        } catch {
            guard !stopped, generation == dialGeneration else {
                if generation == dialGeneration { dialInFlight = false }
                return
            }
            dialInFlight = false
            logger.info("dial mint failed code=\(error.trpcErrorCode ?? "none", privacy: .public)")
            switch error.trpcErrorCode {
            case "NOT_FOUND":
                // The coding_sessions row is gone (stale rows get swept), so
                // the mint answers NOT_FOUND forever AND the row can never
                // report `ended` again — the only other exit from the
                // reconnect loop. Terminal, or the screen would sit at the 30s
                // backoff cap showing a raw error for as long as it stays open.
                phase = .ended(detail: Self.sessionGoneDetail)
            case "FORBIDDEN":
                // Access to the session was revoked (membership/permission) —
                // another permanent no, not a drop.
                phase = .closed(detail: error.trpcUserMessage, reconnecting: false)
            default:
                // Often transient (foregrounding before the network is back) —
                // keep auto-retrying on backoff.
                phase = .closed(
                    detail: "Couldn't get a viewer ticket. \(error.localizedDescription)",
                    reconnecting: true
                )
                scheduleReconnect()
            }
            return
        }
        guard !stopped, generation == dialGeneration else {
            if generation == dialGeneration { dialInFlight = false }
            return
        }
        guard !ticket.isDisabled, let url = ticket.connectURL() else {
            // Config state, not a transient failure — retrying can't help.
            dialInFlight = false
            phase = .closed(detail: "Live sessions are unavailable on this instance.", reconnecting: false)
            return
        }
        logger.info("dial mint ok, opening socket")
        let t = URLSession.shared.webSocketTask(with: url)
        task = t
        connected = true
        t.resume()
        // The feed is NOT wiped here (EXP-249): the relay sends an explicit
        // `activity_reset` to every activity viewer right before its join
        // replay, so the clear happens when the replay actually starts —
        // wiping on dial blanked the screen for the whole ticket+socket
        // round-trip, and left a failed dial showing nothing at all.
        // After a reconnect the replayed transcript event is the ONLY copy of
        // a sent message — it must render, so no stale echo may swallow it.
        recentEchoes = []
        sendText(#"{"t":"join","channel":"activity"}"#)
        armJoinAckDeadline(generation: generation)
        // NOT live yet — the relay may answer the join with no_such_session
        // (the desktop is still starting). The phase flips on the first
        // confirming server frame instead (see markLive()), so the starting /
        // reconnect retry loops never flash the Live header + composer.
        receiveLoop(generation: generation)
    }

    /// EXP-625: bound the wait for the relay's answer to our join. The relay
    /// ALWAYS answers one (apps/steer-relay/src/hub.ts: `activity_reset` plus
    /// the replay, or `error no_such_session` followed by a 4001 close), so
    /// silence past the deadline means a socket that opened and then died
    /// without ever failing a receive. That is the shape of the wedge users hit
    /// after a background, and nothing else in the model notices it.
    private func armJoinAckDeadline(generation: Int) {
        joinAckTask?.cancel()
        joinAckTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.joinAckSeconds))
            guard let self, !Task.isCancelled else { return }
            guard generation == self.dialGeneration, self.connected, self.lastFrameAt == nil else {
                return
            }
            logger.warning("join not answered within \(Self.joinAckSeconds)s, redialing")
            self.disconnectSocket()
            self.onSocketClosed(generation: generation)
        }
    }

    /// Every callback carries the `dialGeneration` its socket belongs to and
    /// goes inert once that generation is superseded: a stale socket's close
    /// would otherwise null out the CURRENT `task` (stopping the new receive
    /// loop from re-arming) and drag a freshly live phase back to `.starting`.
    private func receiveLoop(generation: Int) {
        // The completion runs off the main actor; extract Sendable payloads and
        // hop back (same shape as the deleted SteerViewerModel's loop).
        task?.receive { [weak self] result in
            switch result {
            case .success(.string(let text)):
                Task { @MainActor in
                    guard let self, generation == self.dialGeneration else { return }
                    self.enqueue(text, generation: generation)
                    self.rearm(generation: generation)
                }
            case .success:
                // Stray BINARY frame — the PTY mirror is gone from the
                // protocol (EXP-249), so an old desktop's 0x01 output is the
                // only source left and it is never renderable here.
                Task { @MainActor in
                    guard let self, generation == self.dialGeneration else { return }
                    self.rearm(generation: generation)
                }
            case .failure:
                Task { @MainActor in
                    guard let self, generation == self.dialGeneration else { return }
                    // EXP-621: the relay's close code decides what happens next
                    // (SteerReconnectPolicy) — read it BEFORE onSocketClosed
                    // drops the task. `closeCode` is an imported ObjC enum with
                    // no case for the relay's 4xxx codes, so only its rawValue
                    // is ever touched; anything unrecognizable (0/.invalid)
                    // falls through to the ordinary backoff path.
                    let code = self.task?.closeCode.rawValue
                    self.onSocketClosed(closeCode: code, generation: generation)
                }
            }
        }
    }

    private func rearm(generation: Int) {
        if !stopped, connected, generation == dialGeneration { receiveLoop(generation: generation) }
    }

    /// A frame the relay only sends AFTER a successful join (`activity_reset`
    /// goes out immediately on join; a bad one answers `no_such_session`
    /// instead) — the single proof the room is really live. An open socket
    /// proves nothing:
    /// `resume()` never fails synchronously, so a refusing relay still opens
    /// one, and flipping to live on connect would zero the backoff on every
    /// attempt and redial at ~3s forever instead of walking up to the 30s cap.
    /// Mirrors the Android `FrameResult.live` handling.
    private func markLive() {
        // EXP-773: a room answered, so the journal fetch (if there was one) is
        // over — whatever arrives next IS the transcript.
        history = nil
        // EXP-796: an ENDED run's transcript comes down an open socket too —
        // the relay keeps its history room open for minutes after the
        // device's replay so older pages can still be asked for — and that
        // socket is not a live run. The synced row is the truth: the phase
        // goes straight to `ended` (the header, the composer and every
        // steering gate already read the row), and paging keeps riding the
        // socket until it really closes (`connected`, not the phase, gates
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

    /// Queue a frame and arm one flush for the batch (EXP-582). The flush is a
    /// plain main-actor task: everything the socket delivered before it gets
    /// to run — the whole burst of a replay — is applied together, and a lone
    /// live frame still lands within the same run-loop turn.
    private func enqueue(_ text: String, generation: Int) {
        guard !stopped, connected, generation == dialGeneration else { return }
        // EXP-625: the first frame IS the join answer, so it ends this dial and
        // disarms the deadline. Every frame restamps `lastFrameAt`, which is
        // what a wake signal reads to tell a live socket from a quiet corpse.
        if lastFrameAt == nil {
            let elapsed = dialStartedAt.map { Date().timeIntervalSince($0) } ?? -1
            logger.info("dial answered, first frame in \(elapsed, format: .fixed(precision: 1))s")
            joinAckTask?.cancel()
            joinAckTask = nil
            dialInFlight = false
        }
        lastFrameAt = Date()
        pendingFrames.append(text)
        guard !flushScheduled else { return }
        flushScheduled = true
        Task { @MainActor [weak self] in
            guard let self else { return }
            self.flushScheduled = false
            let frames = self.pendingFrames
            self.pendingFrames = []
            self.applyingBatch = true
            defer {
                self.applyingBatch = false
                self.reproject()
            }
            for frame in frames {
                // A frame can tear the socket down (`no_such_session`); the
                // rest of the batch belonged to that dead room.
                guard self.connected else { return }
                self.onText(frame)
            }
        }
    }

    private func onText(_ text: String) {
        guard let data = text.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let t = obj["t"] as? String else { return }
        // EXP-656: the replay frames go through the pure staging rule first —
        // an `activity_reset` no longer wipes anything, it opens a staging
        // window, and the whole replay swaps in as one commit so the reader
        // never sees the feed collapse to empty and get scrolled back down.
        if let frame = SteerReplayStaging.Frame(wire: t) {
            switch SteerReplayStaging.decide(staging: isStaging, frame: frame) {
            case .beginStaging:
                // Also the join's success signal (EXP-312 — the presence frame
                // that used to confirm it is gone).
                markLive()
                beginStaging()
            case .stage:
                markLive()
                stageFrame(obj["event"] as? [String: Any], seq: obj["seq"] as? Int)
            case .apply:
                markLive()
                currentSeq = obj["seq"] as? Int
                handleActivityEvent(obj["event"] as? [String: Any])
                currentSeq = nil
            case .commit:
                // `activity_synced` is the relay's end-of-replay marker; a
                // `keepalive` (its own 15s beat) proves the burst is over for
                // a republish that carries no marker.
                // EXP-783: the marker also names the SPAN it replayed, so the
                // commit keeps the pages the reader scrolled back to load.
                if frame == .activitySynced {
                    historyTruncated = obj["truncated"] as? Bool == true
                    commitStaging(why: "marker", firstSeq: obj["firstSeq"] as? Int)
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
        // EXP-773: the relay has no live room for this session and is asking
        // the device that ran it to republish its journal. Deliberately NOT a
        // `markLive` — nothing has joined a room yet, and flashing the live
        // header over a finished run would be a lie.
        // EXP-783: one page of OLDER transcript, answering this viewer's
        // `history_page`. PREPENDED, never appended.
        case "history_chunk":
            guard let requestId = obj["requestId"] as? String,
                  requestId == historyRequest else { return }
            if obj["done"] as? Bool == true { historyRequest = nil }
            let events = (obj["events"] as? [[String: Any]]) ?? []
            let seqs = (obj["seqs"] as? [Int]) ?? []
            prependPage(events, seqs: seqs)
        case "history_pending":
            history = .pending
        case "bye":
            let outcome = obj["outcome"] as? String
            if outcome == "publisher_lost" {
                // The desktop's relay socket dropped but the session may still
                // be running — the synced row is the truth. Stay retryable
                // (closed, auto-reconnecting).
                endDetail = "The desktop's connection to the relay dropped. Waiting for it to come back."
            } else {
                sawEnd = true
                // EXP-773: a protocol word is never a caption — `history` (the
                // journal republish closing itself out) and the two history
                // failures leave the banner on its plain "Session ended", and
                // the failures already said their piece through `history`.
                endDetail = SteerOutcome.endDetail(outcome)
            }
        case "error":
            let code = (obj["code"] as? String) ?? "error"
            if code == "no_such_session" {
                // Not live on the relay (yet). With the synced row still
                // running this flips into the auto-retrying starting phase.
                retryStarting = true
                endDetail = "The live stream isn't up yet. The desktop may still be connecting."
                disconnectSocket()
                onSocketClosed()
            } else if code == "device_offline" || code == "history_unavailable" {
                // EXP-773: the transcript lives on the machine that ran the
                // session, and it either can't be reached or has nothing. Both
                // are final — a redial would ask the same question again.
                history = code == "device_offline" ? .deviceOffline : .unavailable
                disconnectSocket()
                onSocketClosed()
            } else {
                endDetail = (obj["message"] as? String) ?? SteerOutcome.endDetail(code)
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
        seqById = [:]
        // The window anchor survives the swap on purpose (EXP-795, web/Android
        // parity): the replayed prefix keeps its ids, so a reader scrolled up
        // stays where they were instead of being yanked to the newest rows on
        // every reconnect.
        latestDiff = nil
        recentEchoes = []
        answerTracker.reset()
        cancelAnswerExpiries()
        clearCompaction()
        // EXP-746: the config/usage slots go too. `resetFeed` only ever runs
        // inside `commitStaging`, which re-applies every staged frame at once,
        // and the relay's join replay always carries the current
        // `config_state`/`usage` after the log (latest-wins kinds) — so the
        // chips repaint inside the same batch and never blank. Keeping a stale
        // config across a session swap would be the actual bug.
        sessionConfig = nil
        sessionUsage = nil
        sessionRateLimit = nil
    }

    // MARK: - Compaction strip (EXP-724)

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
    /// second `activity_reset` means the relay superseded the replay we were
    /// buffering, not that the reader should lose what is on screen.
    private func beginStaging() {
        stagedFrames = []
        stagedLocalEchoes = []
        let now = Date()
        stagingStartedAt = now
        lastStagedFrameAt = now
        armStagingWatcher()
    }

    private func stageFrame(_ event: [String: Any]?, seq: Int?) {
        guard let event else { return }
        stagedFrames?.append((event: event, seq: seq))
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
        resetFeed()
        if let anchorId { nextEventId = anchorId }
        // The retained prefix keeps its rows AND its ids; the replay continues
        // numbering above them, so no row identity is reused.
        if !retained.isEmpty {
            feed = retained
            feedBytes = retained.reduce(0) { $0 + AgentFeed.itemBytes($1) }
            seqById = retainedSeqs
            if let retainedNextId { nextEventId = retainedNextId }
        }
        for staging in staged {
            currentSeq = staging.seq
            handleActivityEvent(staging.event)
        }
        currentSeq = nil
        for text in echoes where !tailCarriesEcho(text) {
            // Not in the replay: re-show it, and re-arm the dedupe so its
            // transcript-derived twin doesn't render a second copy (EXP-78).
            recentEchoes.append((text: text.trimmingCharacters(in: .whitespacesAndNewlines), at: Date()))
            append(.userMessage(id: takeEventId(), text: text))
        }
        for (key, sentAt) in carriedLocks where feedCarriesQuestion(key) {
            restoreAnswerLock(key, labels: carriedLabels[key] ?? [], sentAt: sentAt)
        }
        applyingBatch = nested
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
            guard case let .userMessage(_, existing, _) = item else { continue }
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

    private func handleActivityEvent(_ event: [String: Any]?) {
        guard let event, let kind = event["kind"] as? String else { return }
        switch kind {
        case "narration":
            guard let text = event["text"] as? String,
                  !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            else { return }
            let messageId = Self.trimmedField(event["messageId"])
            let narrationAgent = Self.trimmedField(event["subagentId"])
            // EXP-483: prose from the withheld ask/plan entry flushes AFTER
            // its already-published card — splice it back above the card.
            // The id is taken up front so the spliced row can be stamped
            // with its wire sequence without a scan for "which one is new"
            // (EXP-783); an anchor that matches nothing falls through with it.
            var splicedId: Int?
            if let anchor = Self.trimmedField(event["beforeQuestionId"]) {
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
            if let merged = AgentFeed.mergeNarration(
                feed, text: text, messageId: messageId, subagentId: narrationAgent
            ) {
                feed = merged
                feedBytes += text.utf8.count
                trimFeed()
                return
            }
            append(.narration(
                id: splicedId ?? takeEventId(), text: text,
                messageId: messageId, subagentId: narrationAgent
            ))
        case "tool":
            guard let name = event["name"] as? String else { return }
            append(.tool(
                id: takeEventId(),
                name: name,
                detail: Self.trimmedField(event["detail"]),
                subagentId: Self.trimmedField(event["subagentId"]),
                callId: Self.trimmedField(event["id"]),
                toolKind: AgentFeed.toolKind(event["toolKind"])
            ))
        case "tool_update":
            // EXP-785/786: folded INTO the tool row with that call id — never
            // a row. An id this feed does not hold is dropped.
            guard let next = AgentFeed.applyToolUpdate(feed: feed, event: event) else { return }
            feed = next
            recountFeedBytes()
        case "diff":
            // Diffs never enter the feed — the latest replaces the previous
            // one behind the pinned "Latest changes" chip.
            let diff = event["diff"] as? String
            latestDiff = (diff?.isEmpty == false) ? diff : nil
        case "user_message":
            guard let text = event["text"] as? String,
                  !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
            // A message this client just sent was already echoed locally —
            // skip its transcript-derived twin (EXP-78).
            if consumeEcho(text) { return }
            // EXP-773: a turn addressed to a subagent renders inside that
            // subagent's run, never in the main thread.
            append(.userMessage(
                id: takeEventId(), text: text,
                subagentId: Self.trimmedField(event["subagentId"])
            ))
        case "question":
            guard let question = decodeQuestion(event) else { return }
            // A re-emitted wire id REPLACES its card in place (the desktop
            // augments options it discovers later); anything else appends.
            let before = feed.count
            feed = AgentFeed.upsertQuestion(feed, question: question)
            // Appended: stamp its sequence. Replaced in place: the card's
            // options grew, so re-derive the byte accumulator.
            if feed.count > before, let seq = currentSeq, let id = feed.last?.id {
                seqById[id] = seq
            }
            recountFeedBytes()
        case "question_resolved":
            let id = Self.trimmedField(event["id"])
            let askId = Self.trimmedField(event["askId"])
            let answers = (event["answers"] as? [String]) ?? []
            let dismissed = (event["dismissed"] as? Bool) ?? false
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
        case "answer_ack":
            // The desktop injected the answer — the card stays locked for good
            // and a stepper advances to its next step.
            guard let id = Self.trimmedField(event["id"]) else { return }
            answerTracker.acknowledge(id)
        case "subagent":
            guard let subagentId = Self.trimmedField(event["id"]),
                  let raw = event["status"] as? String,
                  let status = AgentSubagentStatus(rawValue: raw) else { return }
            append(.subagent(
                id: takeEventId(),
                subagentId: subagentId,
                agentType: Self.trimmedField(event["agentType"]) ?? "agent",
                status: status,
                detail: Self.trimmedField(event["detail"]),
                // EXP-748: the publisher's count, stamped on the completed
                // edge — it outlives the tool rows replay evicts first.
                toolCalls: event["toolCalls"] as? Int
            ))
        case "permission":
            guard let tool = Self.trimmedField(event["tool"]) else { return }
            append(.permission(
                id: takeEventId(),
                tool: tool,
                detail: Self.trimmedField(event["detail"])
            ))
        case "config_state":
            // EXP-746: latest-wins STATE, not a row. A malformed frame keeps
            // whatever the chips already show (the fold's contract).
            sessionConfig = AgentFeed.applyConfigState(sessionConfig, event: event)
        case "usage":
            sessionUsage = AgentFeed.applyUsage(sessionUsage, event: event)
        case "rate_limit":
            // EXP-784: the fourth slot; an empty/`ok` status clears it.
            sessionRateLimit = AgentFeed.applyRateLimit(sessionRateLimit, event: event)
        case "compaction":
            // EXP-724. The strip's state is the pure fold; the marker row is
            // the caller's job because only `ended` writes one — and it writes
            // one even for an UNMATCHED `ended` (codex publishes no start
            // marker for auto-compaction, so the gap still gets explained).
            let phase = event["phase"] as? String
            compacting = AgentFeed.applyCompaction(compacting, event: event)
            if phase == "started" {
                armCompactionTimeout(startedAt: event["at"] as? Double)
            } else if phase == "ended" {
                compactionTimeoutTask?.cancel()
                compactionTimeoutTask = nil
                append(.compaction(id: takeEventId()))
            }
        default:
            // Unknown kinds are skipped, never fatal — a newer desktop may
            // publish events this build has no renderer for.
            break
        }
    }

    private func decodeQuestion(_ event: [String: Any]) -> AgentQuestion? {
        // The wire id is required (EXP-613): it addresses the `answer` frame
        // and every resolution event, so an id-less card would be unanswerable
        // and never retire. No publisher emits one.
        guard let wireId = Self.trimmedField(event["id"]),
              let text = event["text"] as? String, !text.isEmpty,
              let rawOptions = event["options"] as? [[String: Any]] else { return nil }
        let options: [AgentQuestionOption] = rawOptions.compactMap { o in
            guard let label = o["label"] as? String, let key = o["key"] as? String,
                  !key.isEmpty else { return nil }
            return AgentQuestionOption(
                label: label, key: key, description: Self.trimmedField(o["description"]),
                freeText: o["freeText"] as? Bool ?? false
            )
        }
        guard !options.isEmpty else { return nil }
        return AgentQuestion(
            id: takeEventId(),
            wireId: wireId,
            askId: Self.trimmedField(event["askId"]),
            index: Self.positiveInt(event["index"]),
            total: Self.positiveInt(event["total"]),
            header: Self.trimmedField(event["header"]),
            text: text,
            options: options,
            multiSelect: (event["multiSelect"] as? Bool) ?? false,
            planMode: (event["planMode"] as? Bool) ?? false
        )
    }

    /// A wire string field, nil unless it carries something.
    private static func trimmedField(_ value: Any?) -> String? {
        guard let text = value as? String,
              !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return text
    }

    private static func positiveInt(_ value: Any?) -> Int? {
        guard let number = value as? NSNumber else { return nil }
        let int = number.intValue
        return int >= 1 ? int : nil
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
        for dropped in feed.prefix(before - trimmed.feed.count) {
            seqById.removeValue(forKey: dropped.id)
        }
        feed = trimmed.feed
        feedBytes = trimmed.bytes
    }

    /// Re-derive the byte accumulator — used after the bulk in-place edits
    /// (a resolved question, a replaced card) whose per-item deltas are not
    /// worth threading through.
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
        joinAckTask?.cancel()
        joinAckTask = nil
        pendingFrames = []
        discardStaging()
        // A page asked for on that socket is not coming either (EXP-795): the
        // relay drops the ask with the viewer, so a new one may go out.
        historyRequest = nil
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
    }

    /// - Parameters:
    ///   - closeCode: the relay's close code when the socket reported one. Nil
    ///     for every teardown this model drives itself (a `no_such_session`
    ///     frame, a failed foreground ping), which keeps their pre-EXP-621
    ///     behavior.
    ///   - generation: the `dialGeneration` the closed socket belonged to. A
    ///     close from a SUPERSEDED dial is inert — its socket is already gone,
    ///     and letting it run would drop the CURRENT dial's task (its receive
    ///     loop then never re-arms) and drag a freshly live phase back. Nil for
    ///     the teardowns this model drives itself against whatever socket is
    ///     current.
    private func onSocketClosed(closeCode: Int? = nil, generation: Int? = nil) {
        if let generation, generation != dialGeneration { return }
        joinAckTask?.cancel()
        joinAckTask = nil
        dialInFlight = false
        guard !stopped else { return }
        logger.info("socket closed code=\(closeCode ?? 0)")
        connected = false
        pendingFrames = []
        discardStaging()
        task = nil
        if sawEnd {
            phase = .ended(detail: endDetail)
            return
        }
        // EXP-773: the device that holds the journal said no (or is away).
        // Terminal on its own terms — the synced row may still read running
        // (a run whose device dropped), and backing off would only re-ask.
        if history?.isTerminal == true {
            phase = .ended(detail: nil)
            return
        }
        if retryStarting, session.map({ CodingSessionLiveness.isLive($0) }) == true {
            // Liveness (not just status) gates the redial — a heartbeat-stale
            // row is a phantom, not a session that's still starting (EXP-153).
            phase = .starting
            scheduleStartingRetry()
            return
        }
        switch SteerReconnectPolicy.decide(closeCode: closeCode, sessionOver: sessionEnded) {
        case .ended:
            // The row already says ended — the redial loops exit on exactly
            // this, so skip straight to the end state instead of scheduling a
            // retry whose only job is to notice it.
            phase = .ended(detail: nil)
        case .redialImmediately:
            redialNow()
        case .terminalClosed:
            // The relay refused this viewer (4003). Backoff would just
            // re-refuse; the mint's own FORBIDDEN path is equally terminal.
            phase = .closed(
                detail: "You're no longer authorized for this session.", reconnecting: false
            )
        case .reconnectWithBackoff:
            // Never park on a dead socket behind a manual button (EXP-243) —
            // auto-redial on backoff; the phase carries the reconnecting flag
            // so the UI shows "Reconnecting…" instead of a Reconnect action.
            phase = .closed(detail: endDetail ?? "Connection lost.", reconnecting: true)
            scheduleReconnect()
        }
    }

    /// EXP-621: redial without touching the phase — the relay evicted a slow
    /// consumer (4008), which says nothing about the session, the ticket or the
    /// network. Flipping to `.closed(reconnecting:)` would flash a
    /// "Reconnecting…" banner and hide the composer for a round trip, and
    /// counting it as a failed attempt would walk the backoff curve up towards
    /// 30s for a drop the client caused. The join replay repaints the feed.
    private func redialNow() {
        retryTask?.cancel()
        reconnectAttempts = 0
        retryTask = Task { [weak self] in
            guard let self, !self.stopped, !Task.isCancelled else { return }
            self.resetDialState()
            await self.dial()
        }
    }

    /// Redial (fresh ticket) after ~3s while the desktop is still starting —
    /// the phase stays `.starting` so the header doesn't flicker.
    private func scheduleStartingRetry() {
        retryTask?.cancel()
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.startingRetrySeconds))
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

    /// Auto-redial (fresh ticket) after an unexpected drop (EXP-243) — the
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
        guard connected, let task else { return }
        task.send(.string(text)) { _ in }
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
