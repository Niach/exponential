import ExpCore
import Foundation
import GRDB

/// One watcher in the relay room (relay `presence` frames).
struct AgentPresenceViewer: Identifiable, Equatable {
    let userId: String
    let name: String
    let perm: String
    var id: String { userId }
}

/// Viewer side of the steer relay's ACTIVITY channel (EXP-32 — the chat-style
/// "Agent session" screen; apps/steer-relay/src/protocol.ts). Mints a viewer
/// ticket over tRPC, dials the returned ws(s) URL with URLSessionWebSocketTask,
/// joins with {"t":"join","channel":"activity"}, and receives scrubbed
/// {t:'activity', event} frames (narration / tool headlines / questions /
/// subagents / permissions / worktree diffs). EXP-249 removed the PTY mirror
/// from the protocol, so every frame is JSON now — a stray BINARY frame (an old
/// desktop's 0x01 output) is ignored. Steering is message-shaped: a steal-claim
/// + chunked input + a separate \r for prose, and ONE semantic `answer` frame
/// per question card. Mirrors the Android AgentSessionViewModel.
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

    private(set) var phase: Phase = .idle
    /// The feed stays visible while disconnected (closed/ended states) and is
    /// cleared ONLY by the relay's `activity_reset` frame (EXP-249) — the relay
    /// sends one to every activity viewer immediately before its join replay,
    /// so the client never has to guess when to wipe. Item shapes and the pure
    /// grouping/resolution rules live in ExpCore's AgentFeed.
    private(set) var feed: [AgentFeedItem] = []
    /// Per-card answer lock (EXP-249): a tap locks its card immediately, the
    /// desktop's `answer_ack` makes that permanent (and advances a stepper),
    /// and an unanswered optimistic lock expires so the card stays retryable.
    private(set) var answerTracker = AgentAnswerTracker()
    /// The most recent worktree diff — each one replaces the previous.
    private(set) var latestDiff: String?
    private(set) var viewers: [AgentPresenceViewer] = []
    private(set) var steererId: String?
    /// The minted ticket's perm claim (`view`/`steer`), display-gating only —
    /// the relay enforces it server-side regardless.
    private(set) var perm = "view"
    /// The synced coding_sessions row — flips to ended via Electric.
    private(set) var session: CodingSessionEntity?
    /// Kill-switch failure (EXP-268), surfaced as an inline banner — cleared
    /// on each attempt. Success needs no local state: the synced row flips to
    /// `ended` and the view already reacts.
    private(set) var killError: String?

    var canSteer: Bool { perm == "steer" }
    var isSteering: Bool { steererId != nil && steererId == currentUserId }
    var remoteSteererName: String? {
        guard let steererId, steererId != currentUserId else { return nil }
        return viewers.first { $0.userId == steererId }?.name ?? "Someone"
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

    /// Whether this viewer may kill the session (EXP-268): a live (not-ended)
    /// synced row, and either the steer perm (session owner / team owner by
    /// ticket) or the session's own runner. Display gating only — the server
    /// enforces the same rule again.
    var canKill: Bool {
        guard !sessionEnded else { return false }
        if canSteer { return true }
        guard let currentUserId else { return false }
        return session?.userId == currentUserId
    }

    /// Ids of the question items still answerable — the pure rule lives in
    /// ExpCore (`AgentFeed.activeQuestionIds`), mirroring the Android
    /// `activeQuestionIds`.
    var activeQuestionIds: Set<Int> { AgentFeed.activeQuestionIds(feed) }

    /// Render rows: subagent runs, multi-question ask steppers, and runs of ≥2
    /// consecutive tool calls collapse (EXP-97/EXP-249) — a projection only,
    /// the flat feed stays the state.
    var rows: [AgentFeedRow] { AgentFeed.rows(feed) }

    /// Questions whose answer is out — sent (optimistic lock) or confirmed
    /// (`answer_ack`). What a stepper card advances on (web parity: advance on
    /// send; the 5s no-ack expiry rolls the step back).
    var answeredQuestionIds: Set<String> { answerTracker.lockedKeys }

    /// Whether a card is locked against further taps (sent-and-unconfirmed, or
    /// confirmed by the desktop).
    func isAnswerLocked(_ lockKey: String) -> Bool { answerTracker.isLocked(lockKey) }

    /// An answer went out for this card but the desktop hasn't confirmed
    /// injecting it yet.
    func isAnswerPending(_ lockKey: String) -> Bool { answerTracker.isPending(lockKey) }

    /// Live but blocked on a trailing question/plan — the session is waiting
    /// for a human answer, not stuck (EXP-97).
    var awaitingInput: Bool { phase == .live && !activeQuestionIds.isEmpty }

    private let accountId: String
    private let codingSessionId: String
    private let currentUserId: String?
    private let steerApi: SteerApi
    private let db: DatabaseManager

    private var task: URLSessionWebSocketTask?
    private var connected = false
    private var stopped = false
    private var sawEnd = false
    private var retryStarting = false
    private var endDetail: String?
    private var nextEventId = 0
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
    private var idleReleaseTask: Task<Void, Never>?
    private var sessionObservationTask: Task<Void, Never>?
    private var answerExpiryTask: Task<Void, Never>?

    // Relay rejects input frames > 8 KiB; chunk pastes well under that.
    private static let inputChunkChars = 4096
    /// How long an optimistic answer lock holds without an `answer_ack` or a
    /// `question_resolved` — long enough to cover a slow desktop injection,
    /// short enough that a lost frame doesn't strand the card. Web/Android
    /// parity (ANSWER_ACK_TIMEOUT_MS, EXP-249).
    private static let answerLockSeconds: Double = 5
    /// Auto-release the steer claim after this long with no sends.
    private static let idleReleaseSeconds: Double = 60
    /// Redial cadence while the desktop's publisher socket is still starting.
    private static let startingRetrySeconds: Double = 3
    /// Auto-reconnect backoff after an unexpected drop (EXP-243): jittered
    /// exponential 3s→30s, mirroring the web viewer's starting retry — the
    /// jitter desyncs a herd of viewers all foregrounding at once.
    private static let reconnectBaseSeconds: Double = 3
    private static let reconnectMaxSeconds: Double = 30
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
        db: DatabaseManager
    ) {
        self.accountId = accountId
        self.codingSessionId = session.id
        self.currentUserId = currentUserId
        self.steerApi = steerApi
        self.db = db
        self.session = session
    }

    /// Bind the synced session row and auto-connect once when presented;
    /// drops after that auto-reconnect (EXP-243).
    func start() {
        startObservingSession()
        if phase == .idle { connect() }
    }

    /// Dial (or re-dial, with a fresh ticket) the relay room.
    func connect() {
        guard phase != .connecting, phase != .live else { return }
        stopped = false
        retryTask?.cancel()
        reconnectAttempts = 0
        resetDialState()
        phase = .connecting
        Task { await dial() }
    }

    /// Foreground revival (EXP-243): the socket rarely survives app
    /// suspension. Skip any pending reconnect backoff and redial now; while
    /// nominally live, ping-probe the socket so a connection that died in the
    /// background surfaces (and auto-reconnects) immediately instead of on
    /// the next failed receive.
    func reconnectNow() {
        guard !stopped else { return }
        if case .closed(_, true) = phase {
            connect()
        } else if phase == .live {
            task?.sendPing { [weak self] error in
                guard error != nil else { return }
                Task { @MainActor in
                    guard let self, self.connected else { return }
                    self.disconnectSocket()
                    self.onSocketClosed()
                }
            }
        }
    }

    /// Revive after a shutdown() (EXP-221): as a pushed destination the view
    /// stays in the navigation stack while e.g. a deep link covers it —
    /// onDisappear tears the socket down, so popping back must re-arm the
    /// session observation and redial (the relay replays the full activity
    /// log on join, rebuilding the feed).
    func resume() {
        guard stopped else { return }
        startObservingSession()
        connect()
    }

    /// Tear everything down (view dismissed): best-effort claim release, then
    /// close. Closing the socket also releases the claim relay-side.
    func shutdown() {
        stopped = true
        releaseNow()
        retryTask?.cancel()
        idleReleaseTask?.cancel()
        answerExpiryTask?.cancel()
        sessionObservationTask?.cancel()
        sessionObservationTask = nil
        connected = false
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
    /// through the relay so the terminal tears down immediately. On success
    /// nothing changes locally — the synced row flips and the view reacts.
    func killSession() async {
        killError = nil
        do {
            try await steerApi.killSession(accountId: accountId, codingSessionId: codingSessionId)
        } catch {
            killError = error.trpcUserMessage
        }
    }

    // MARK: - Steering (message-shaped; the relay enforces the single claim)

    /// Send one message to the agent: ALWAYS steal-claim first (the relay
    /// gates input per CONNECTION while presence only exposes the steerer's
    /// USER id — if this user holds the claim on another socket, e.g. the web
    /// steer terminal or a second phone, skipping the claim would make the
    /// relay silently drop every frame; the claim is idempotent for the
    /// current holder and last-writer-wins by design), then forward the text
    /// (chunked ≤4 KiB), then a SEPARATE `\r` frame — bundled into one write
    /// TUI apps treat the trailing return as a paste, which inserts instead
    /// of submitting.
    func sendMessage(_ text: String) {
        guard !text.isEmpty, canSteer, connected else { return }
        sendText(#"{"t":"claim","steal":true}"#)
        var rest = Substring(text)
        while !rest.isEmpty {
            let chunk = String(rest.prefix(Self.inputChunkChars))
            rest = rest.dropFirst(chunk.count)
            let frame: [String: Any] = ["t": "input", "data": chunk]
            if let data = try? JSONSerialization.data(withJSONObject: frame),
               let json = String(data: data, encoding: .utf8) {
                sendText(json)
            }
        }
        sendText(#"{"t":"input","data":"\r"}"#)
        scheduleIdleRelease()
        // Local echo (EXP-78): show the sent message immediately; its
        // transcript-derived `user_message` event is deduped via the FIFO.
        recentEchoes.append((text: text.trimmingCharacters(in: .whitespacesAndNewlines), at: Date()))
        if recentEchoes.count > Self.echoCap {
            recentEchoes.removeFirst(recentEchoes.count - Self.echoCap)
        }
        append(.userMessage(id: takeEventId(), text: text))
    }

    /// Answer a protocol-v2 question card (EXP-249): steal-claim first (same
    /// per-CONNECTION reason as sendMessage), then ONE semantic `answer` frame
    /// carrying every chosen key — the desktop owns the keystroke mapping and
    /// confirms the injection with `answer_ack`. The card locks the moment the
    /// frame goes out, so a double tap can never answer twice.
    func sendAnswer(questionId: String, askId: String?, keys: [String]) {
        guard !questionId.isEmpty, !keys.isEmpty, canSteer, connected else { return }
        guard !answerTracker.isLocked(questionId) else { return }
        sendText(#"{"t":"claim","steal":true}"#)
        var frame: [String: Any] = ["t": "answer", "questionId": questionId, "keys": keys]
        if let askId, !askId.isEmpty { frame["askId"] = askId }
        if let data = try? JSONSerialization.data(withJSONObject: frame),
           let json = String(data: data, encoding: .utf8) {
            sendText(json)
        }
        lockAnswer(questionId)
        scheduleIdleRelease()
    }

    /// Answer a LEGACY question card (a desktop that publishes no question ids,
    /// so there is nothing to address a semantic `answer` frame to): steal-claim
    /// + the raw TUI keystroke, which the desktop passes to the PTY unwrapped.
    /// NOTHING follows the digit — the old trailing `\r` submitted the picker a
    /// second time and cascaded into the NEXT question (EXP-249). `lockKey`
    /// locks the card; multi-select toggles pass nil (each digit only toggles,
    /// `sendLegacyAdvance` submits).
    func sendLegacyKey(_ key: String, lockKey: String? = nil) {
        guard !key.isEmpty, canSteer, connected else { return }
        if let lockKey, answerTracker.isLocked(lockKey) { return }
        sendText(#"{"t":"claim","steal":true}"#)
        let frame: [String: Any] = ["t": "input", "data": key]
        if let data = try? JSONSerialization.data(withJSONObject: frame),
           let json = String(data: data, encoding: .utf8) {
            sendText(json)
        }
        if let lockKey { lockAnswer(lockKey) }
        scheduleIdleRelease()
    }

    /// Advance a legacy multi-select picker. Tab, NOT Enter: with the cursor on
    /// an option row Enter TOGGLES it (verified against claude v2.1.215 —
    /// silently corrupting the selection), while Tab moves to the next
    /// tab/review step, whose picker the grid watcher publishes as its own card
    /// (EXP-197).
    func sendLegacyAdvance(lockKey: String?) {
        sendLegacyKey("\t", lockKey: lockKey)
    }

    /// Lock a card and arm the expiry that frees it again if neither an
    /// `answer_ack` nor a `question_resolved` ever lands.
    private func lockAnswer(_ lockKey: String) {
        answerTracker.markSent(lockKey)
        answerExpiryTask?.cancel()
        answerExpiryTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.answerLockSeconds))
            guard let self, !Task.isCancelled else { return }
            self.answerTracker.expire(timeout: Self.answerLockSeconds)
        }
    }

    /// Best-effort claim release — closing the socket also releases it
    /// relay-side; this just makes it prompt.
    func releaseNow() {
        idleReleaseTask?.cancel()
        guard isSteering else { return }
        sendText(#"{"t":"release"}"#)
    }

    /// Auto-release the claim after 60s of no sends (timer resets per send).
    private func scheduleIdleRelease() {
        idleReleaseTask?.cancel()
        idleReleaseTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.idleReleaseSeconds))
            guard !Task.isCancelled else { return }
            self?.releaseNow()
        }
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
            do {
                for try await row in observation.values(in: pool) {
                    guard let self else { continue }
                    // A nil row is published (not swallowed): the row is gone,
                    // which `sessionEnded` reads as ended. Holding the last
                    // known copy instead left the retry loops waiting for an
                    // `ended` status a deleted row can never report.
                    self.session = row
                }
            } catch {}
        }
    }

    // MARK: - Connect lifecycle

    private func resetDialState() {
        sawEnd = false
        retryStarting = false
        endDetail = nil
        viewers = []
        steererId = nil
    }

    private func dial() async {
        dialGeneration += 1
        let generation = dialGeneration
        let ticket: SteerTicket
        do {
            ticket = try await steerApi.mintViewerTicket(accountId: accountId, codingSessionId: codingSessionId)
        } catch {
            guard !stopped, generation == dialGeneration else { return }
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
        guard !stopped, generation == dialGeneration else { return }
        guard !ticket.isDisabled, let url = ticket.connectURL() else {
            // Config state, not a transient failure — retrying can't help.
            phase = .closed(detail: "Live sessions are unavailable on this instance.", reconnecting: false)
            return
        }
        perm = Self.decodeTicketPerm(ticket.ticket ?? "")

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
        // NOT live yet — the relay may answer the join with no_such_session
        // (the desktop is still starting). The phase flips on the first
        // confirming server frame instead (see markLive()), so the starting /
        // reconnect retry loops never flash the Live header + composer.
        receiveLoop()
    }

    private func receiveLoop() {
        // The completion runs off the main actor; extract Sendable payloads and
        // hop back (same shape as the deleted SteerViewerModel's loop).
        task?.receive { [weak self] result in
            switch result {
            case .success(.string(let text)):
                Task { @MainActor in
                    self?.onText(text)
                    self?.rearm()
                }
            case .success:
                // Stray BINARY frame — the PTY mirror is gone from the
                // protocol (EXP-249), so an old desktop's 0x01 output is the
                // only source left and it is never renderable here.
                Task { @MainActor in self?.rearm() }
            case .failure:
                Task { @MainActor in self?.onSocketClosed() }
            }
        }
    }

    private func rearm() {
        if !stopped, connected { receiveLoop() }
    }

    /// A frame the relay only sends AFTER a successful join (presence goes out
    /// immediately on join; a bad one answers `no_such_session` instead) — the
    /// single proof the room is really live. An open socket proves nothing:
    /// `resume()` never fails synchronously, so a refusing relay still opens
    /// one, and flipping to live on connect would zero the backoff on every
    /// attempt and redial at ~3s forever instead of walking up to the 30s cap.
    /// Mirrors the Android `FrameResult.live` handling.
    private func markLive() {
        guard phase != .live else { return }
        phase = .live
        reconnectAttempts = 0
    }

    private func onText(_ text: String) {
        guard let data = text.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let t = obj["t"] as? String else { return }
        switch t {
        case "presence":
            markLive()
            let raw = obj["viewers"] as? [[String: Any]] ?? []
            viewers = raw.compactMap { v in
                guard let userId = v["userId"] as? String else { return nil }
                return AgentPresenceViewer(
                    userId: userId,
                    name: (v["name"] as? String) ?? userId,
                    perm: (v["perm"] as? String) ?? "view"
                )
            }
            steererId = obj["steererId"] as? String
        case "activity":
            markLive()
            handleActivityEvent(obj["event"] as? [String: Any])
        case "activity_reset":
            // The ONLY feed wipe (EXP-249): the relay sends one immediately
            // before every join replay, and again whenever a publisher restarts
            // its stream — so a replay rebuilds the feed instead of appending a
            // duplicate copy of the whole history.
            resetFeed()
        case "bye":
            let outcome = obj["outcome"] as? String
            if outcome == "publisher_lost" {
                // The desktop's relay socket dropped but the session may still
                // be running — the synced row is the truth. Stay retryable
                // (closed, auto-reconnecting).
                endDetail = "The desktop's connection to the relay dropped — waiting for it to come back."
            } else {
                sawEnd = true
                endDetail = (outcome != nil && outcome != "ended") ? outcome : nil
            }
        case "error":
            let code = (obj["code"] as? String) ?? "error"
            if code == "no_such_session" {
                // Not live on the relay (yet). With the synced row still
                // running this flips into the auto-retrying starting phase.
                retryStarting = true
                endDetail = "The live stream isn't up yet — the desktop may still be connecting."
                disconnectSocket()
                onSocketClosed()
            } else {
                endDetail = (obj["message"] as? String) ?? code
            }
        default:
            break // input/claim/kill — not activity-viewer-relevant
        }
    }

    /// Drop everything the room's activity log owns. Local echoes go too: the
    /// replay that follows carries its own copy of every sent message.
    private func resetFeed() {
        feed = []
        latestDiff = nil
        recentEchoes = []
        answerTracker.reset()
        answerExpiryTask?.cancel()
        // nextEventId keeps counting — reused render ids across a reset would
        // hand SwiftUI a "same row, new content" identity.
    }

    private func handleActivityEvent(_ event: [String: Any]?) {
        guard let event, let kind = event["kind"] as? String else { return }
        switch kind {
        case "narration":
            guard let text = event["text"] as? String else { return }
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { return }
            // LEGACY resolution signals (EXP-197): a protocol-v2 desktop emits
            // them BESIDE `question_resolved` purely for old clients, so they
            // are dropped entirely once the feed carries question ids. On a
            // legacy feed they fold into the pending card instead of rendering
            // as a narration row — with no card waiting the answer still
            // renders, so it is never lost.
            let semantic = AgentFeed.hasSemanticQuestions(feed)
            if trimmed.hasPrefix(AgentFeed.questionAnsweredPrefix) {
                guard !semantic else { return }
                let answer = String(trimmed.dropFirst(AgentFeed.questionAnsweredPrefix.count))
                if let out = AgentFeed.attachQuestionAnswer(feed, answer: answer) {
                    feed = out
                    return
                }
                // No legacy card waiting — fall through so the answer still shows.
            } else if trimmed == AgentFeed.questionDismissedNarration {
                guard !semantic else { return }
                if let out = AgentFeed.dismissPendingQuestions(feed) { feed = out }
                return
            } else if trimmed == AgentFeed.planResolvedNarration, semantic {
                // Legacy feeds need it IN the feed — `activeQuestionIds` reads
                // it as the plan card's only retirement signal.
                return
            }
            append(.narration(id: takeEventId(), text: text))
        case "tool":
            guard let name = event["name"] as? String else { return }
            append(.tool(
                id: takeEventId(),
                name: name,
                detail: Self.trimmedField(event["detail"]),
                subagentId: Self.trimmedField(event["subagentId"])
            ))
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
            append(.userMessage(id: takeEventId(), text: text))
        case "question":
            guard let question = decodeQuestion(event) else { return }
            // A re-emitted wire id REPLACES its card in place (the desktop
            // augments options it discovers later); anything else appends.
            feed = AgentFeed.upsertQuestion(feed, question: question)
            trimFeed()
        case "question_resolved":
            let id = Self.trimmedField(event["id"])
            let askId = Self.trimmedField(event["askId"])
            let answers = (event["answers"] as? [String]) ?? []
            let dismissed = (event["dismissed"] as? Bool) ?? false
            // Collect the retiring cards' lock keys BEFORE they resolve —
            // a retired card has nothing left for the optimistic lock to guard.
            let retiredKeys: [String] = feed.compactMap { item -> String? in
                guard let question = item.question, !question.resolved else { return nil }
                if let id { return question.wireId == id ? question.lockKey : nil }
                if let askId { return question.askId == askId ? question.lockKey : nil }
                return question.lockKey
            }
            if let out = AgentFeed.applyQuestionResolved(
                feed, id: id, askId: askId, answers: answers, dismissed: dismissed
            ) {
                feed = out
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
                detail: Self.trimmedField(event["detail"])
            ))
        case "permission":
            guard let tool = Self.trimmedField(event["tool"]) else { return }
            append(.permission(
                id: takeEventId(),
                tool: tool,
                detail: Self.trimmedField(event["detail"])
            ))
        default:
            // Unknown kinds are skipped, never fatal — a newer desktop may
            // publish events this build has no renderer for.
            break
        }
    }

    private func decodeQuestion(_ event: [String: Any]) -> AgentQuestion? {
        guard let text = event["text"] as? String, !text.isEmpty,
              let rawOptions = event["options"] as? [[String: Any]] else { return nil }
        let options: [AgentQuestionOption] = rawOptions.compactMap { o in
            guard let label = o["label"] as? String, let key = o["key"] as? String,
                  !key.isEmpty else { return nil }
            return AgentQuestionOption(
                label: label, key: key, description: Self.trimmedField(o["description"])
            )
        }
        guard !options.isEmpty else { return nil }
        return AgentQuestion(
            id: takeEventId(),
            wireId: Self.trimmedField(event["id"]),
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
        feed.append(item)
        trimFeed()
    }

    /// Old events fall off the top at the relay's own log cap, so a full replay
    /// is never truncated client-side (EXP-249).
    private func trimFeed() {
        if feed.count > AgentFeed.feedCap {
            feed.removeFirst(feed.count - AgentFeed.feedCap)
        }
    }

    private func disconnectSocket() {
        connected = false
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
    }

    private func onSocketClosed() {
        guard !stopped else { return }
        connected = false
        task = nil
        idleReleaseTask?.cancel()
        viewers = []
        steererId = nil
        if sawEnd {
            phase = .ended(detail: endDetail)
        } else if retryStarting, session.map({ CodingSessionLiveness.isLive($0) }) == true {
            // Liveness (not just status) gates the redial — a heartbeat-stale
            // row is a phantom, not a session that's still starting (EXP-153).
            phase = .starting
            scheduleStartingRetry()
        } else {
            // Never park on a dead socket behind a manual button (EXP-243) —
            // auto-redial on backoff; the phase carries the reconnecting flag
            // so the UI shows "Reconnecting…" instead of a Reconnect action.
            phase = .closed(detail: endDetail ?? "Connection lost.", reconnecting: true)
            scheduleReconnect()
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
                self.phase = .ended(detail: nil)
                return
            }
            self.resetDialState()
            await self.dial()
        }
    }

    /// Auto-redial (fresh ticket) after an unexpected drop (EXP-243) — the
    /// phase stays `.closed(reconnecting: true)` across the wait and the dial
    /// so the banner doesn't flicker; a foreground reconnectNow() cancels the
    /// wait and dials immediately.
    private func scheduleReconnect() {
        retryTask?.cancel()
        let delay = Self.reconnectDelay(attempt: reconnectAttempts)
        reconnectAttempts += 1
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard let self, !self.stopped, !Task.isCancelled else { return }
            if self.sessionEnded {
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

    /// The ticket is `base64url(JSON claims).base64url(sig)` — decode the perm
    /// claim for display gating (the relay enforces it server-side regardless).
    static func decodeTicketPerm(_ ticket: String) -> String {
        guard let dot = ticket.firstIndex(of: ".") else { return "view" }
        var b64 = String(ticket[..<dot])
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        while b64.count % 4 != 0 { b64 += "=" }
        guard let data = Data(base64Encoded: b64),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let perm = obj["perm"] as? String else { return "view" }
        return perm == "steer" ? "steer" : "view"
    }
}
